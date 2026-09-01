use serde::{Deserialize, Serialize};
mod snowball;
use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::{self, BufReader, BufWriter},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{Arc, Mutex},
    thread,
    time::SystemTime,
};
use tiny_http::{Header, Method, Request, Response, Server};
use xml::reader::{EventReader, XmlEvent::Characters};
mod lexer;
use lexer::Lexer;
type TF = HashMap<String, usize>;
type DocFreq = HashMap<String, usize>;
type Docs = HashMap<PathBuf, Doc>;

#[derive(Deserialize, Serialize, Debug)]
struct Doc {
    tf: TF,
    count: usize,
    last_modified: SystemTime,
}

#[derive(Default, Deserialize, Serialize)]
struct Model {
    df: DocFreq,
    docs: Docs,
}

fn read_entire_xml_file<P: AsRef<Path>>(file_path: P) -> io::Result<String> {
    let file = File::open(file_path)?;
    let read_buf = BufReader::new(file);
    let er = EventReader::new(read_buf);
    let mut content = String::new();
    for event in er.into_iter() {
        if let Characters(text) = event.expect("TODO") {
            content.push_str(&text);
            content.push_str(" ");
        }
    }
    Ok(content)
}

fn usage(program: &str) {
    eprintln!("Usage: {program} [COMMAND] [OPTIONS]");
    eprintln!("Command:");
    eprintln!("    serve <folder> [address]       start local HTTP server with Web Interface");
}

fn serve_static_file(request: Request, file_path: &str, content_type: &str) -> Result<(), ()> {
    let response = Response::from_file(
        File::open(file_path).map_err(|err| eprintln!("Could not open file : {err}"))?,
    )
    .with_header(
        Header::from_bytes("Content-Type", content_type).map_err(|err| {
            eprintln!("Could not make header with content_type {content_type} : {err:?}")
        })?,
    );
    request.respond(response).map_err(|err| {
        eprintln!("Could not serve static file {file_path} : {err}");
    })
}

fn serve_404(request: Request) -> Result<(), ()> {
    request
        .respond(Response::from_string("404").with_status_code(404))
        .map_err(|err| {
            eprintln!("Could not serve 404 : {err}");
        })
}

fn tf(t: &str, doc: &Doc) -> f32 {
    *doc.tf.get(t).unwrap_or(&0) as f32 / doc.count as f32
}

fn idf(t: &str, n: usize, d: &DocFreq) -> f32 {
    let n = n as f32;
    let count = *d.get(t).unwrap_or(&0) as f32;

    ((n + 1.0) / (count + 1.0)).log10() + 1.0
}

fn search_for_best_document(mut request: Request, model: &Arc<Mutex<Model>>) -> Result<(), ()> {
    let mut buf = Vec::new();
    let _ = request.as_reader().read_to_end(&mut buf);
    let body = str::from_utf8(&buf)
        .map_err(|err| eprintln!("Can not get body : {err}"))?
        .chars()
        .collect::<Vec<_>>();
    let mut result = Vec::<(&Path, f32)>::new();

    let model = model.lock().unwrap();
    for (path, doc) in &model.docs {
        let mut total_tf_idf: f32 = 0.0;
        for token in Lexer::new(&body) {
            total_tf_idf += tf(&token, doc) * idf(&token, model.docs.len(), &model.df);
        }
        result.push((path, total_tf_idf));
    }

    result.sort_by(|a, b| b.1.total_cmp(&a.1));
    let response_string = result
        .iter()
        .take(10)
        .map(|res| res.0.to_string_lossy())
        .collect::<Vec<_>>()
        .join("\n");

    request
        .respond(Response::from_string(response_string))
        .map_err(|err| eprintln!("Can not make response : {err}"))?;

    Ok(())
}

fn serve_request(model: &Arc<Mutex<Model>>, request: Request) -> Result<(), ()> {
    println!(
        "receiver request method: {:?}, url:{:?}",
        request.method(),
        request.url(),
    );
    match (request.method(), request.url()) {
        (Method::Post, "/api/search") => {
            let _ = search_for_best_document(request, model);
        }

        (Method::Get, "/") | (Method::Get, "/index.html") => {
            let _ = serve_static_file(request, "index.html", "text/html; charset=UTF-8");
        }

        (Method::Get, "/index.js") => {
            let _ = serve_static_file(request, "index.js", "text/javascript; charset=UTF-8");
        }

        _ => {
            let _ = serve_404(request);
        }
    }

    Ok(())
}

fn entry() -> Result<(), ()> {
    let mut args = env::args();
    let program = args.next().expect("path to program is provided");

    let subcommand = args.next().ok_or_else(|| {
        usage(&program);
        eprintln!("ERROR: no subcommand is provided");
    })?;

    match subcommand.as_str() {
        "serve" => {
            let folder_path = args.next().ok_or_else(|| {
                usage(&program);
                eprintln!("ERROR: no path to folder is provided");
            })?;
            let index_path = "index.json";
            let model: Arc<Mutex<Model>> = Arc::new(Mutex::new(
                read_index(&index_path).unwrap_or(Default::default()),
            ));

            let model_clone = Arc::clone(&model);
            thread::spawn(move || {
                let _ = create_index(&folder_path, Some(model_clone), index_path);
            });
            let address = args.next().unwrap_or("127.0.0.1:6969".to_string());
            let server = Server::http(&address)
                .map_err(|e| eprintln!("Error: could not start http server at {address} : {e}"))?;

            println!("Listening at {address}");
            for request in server.incoming_requests() {
                let _ = serve_request(&model, request);
            }
            return Ok(());
        }

        _ => {
            usage(&program);
            eprintln!("ERROR: unknown subcommand {subcommand}");
            Err(())
        }
    }
}

fn main() -> ExitCode {
    match entry() {
        Ok(()) => ExitCode::SUCCESS,
        Err(()) => ExitCode::FAILURE,
    }
}

fn read_index(index_path: &str) -> Result<Model, ()> {
    let index_file =
        File::open(index_path).map_err(|err| eprintln!("Can not open index file : {err}"))?;
    let read_buf = BufReader::new(index_file);
    serde_json::from_reader(read_buf).map_err(|err| {
        eprintln!("Index file can not be read: {err}");
    })
}

fn remove_document(model: &Arc<Mutex<Model>>, file_path: &Path) {
    let mut model = model.lock().unwrap();
    if let Some(doc) = model.docs.remove(file_path) {
        for t in doc.tf.keys() {
            if let Some(f) = model.df.get_mut(t) {
                *f -= 1;
            }
        }
    }
}

fn add_document(
    model: &Arc<Mutex<Model>>,
    file_path: PathBuf,
    last_modified: SystemTime,
    content: &Vec<char>,
) -> Result<(), ()> {
    let mut tf = TF::new();
    let mut count = 0;
    for token in Lexer::new(&content) {
        *tf.entry(token).or_insert(0) += 1;
        count += 1;
    }
    let mut model = model.lock().unwrap();
    for t in tf.keys() {
        if let Some(freq) = model.df.get_mut(t) {
            *freq += 1;
        } else {
            model.df.insert(t.to_string(), 1);
        }
    }
    let mut stats = tf.iter().collect::<Vec<_>>();
    stats.sort_by_key(|(_, f)| **f);
    stats.reverse();

    model.docs.insert(
        file_path,
        Doc {
            count,
            tf,
            last_modified,
        },
    );

    Ok(())
}

fn requires_reindex(
    model: &Arc<Mutex<Model>>,
    file_path: &PathBuf,
    last_modified: SystemTime,
) -> bool {
    let model = model.lock().unwrap();
    if let Some(doc) = model.docs.get(file_path) {
        if doc.last_modified >= last_modified {
            return false;
        }
    }
    return true;
}

fn index_folder(model: &Arc<Mutex<Model>>, dir_path: &Path) -> Result<(), io::Error> {
    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let file_path = entry.path();
        let last_modified = entry.metadata()?.modified()?;
        if file_path.is_dir() {
            index_folder(&model, &file_path)?;
        } else {
            if file_path
                .extension()
                .is_some_and(|ext| ext == "xhtml" || ext == "pdf")
            {
                let need_reindex = requires_reindex(&model, &file_path, last_modified);
                if need_reindex {
                    println!("Indexing {file_path:?}");
                    remove_document(&model, &file_path);
                    let content;
                    if file_path.extension().is_some_and(|ext| ext == "xhtml") {
                        content = read_entire_xml_file(&file_path)?
                            .chars()
                            .collect::<Vec<_>>();
                    } else {
                        let bytes = std::fs::read(&file_path)?;

                        match pdf_extract::extract_text_from_mem(&bytes) {
                            Ok(text) => content = text.chars().collect::<Vec<_>>(),
                            Err(e) => {
                                eprintln!("Error extracting text: {}", e);
                                continue;
                            }
                        }
                    }
                    add_document(&model, file_path, last_modified, &content).map_err(|_| {
                        std::io::Error::new(std::io::ErrorKind::Other, "add_document failed")
                    })?;
                } else {
                    println!("Skip indexing {file_path:?}");
                }
            }
        }
    }
    Ok(())
}

fn create_index(
    folder_path: &str,
    model: Option<Arc<Mutex<Model>>>,
    index_path: &str,
) -> Result<(), ()> {
    let model: Arc<Mutex<Model>> = model.unwrap_or(Default::default());
    let dir_path = Path::new(folder_path);

    index_folder(&model, dir_path).map_err(|err| eprintln!("Can not create index : {err}"))?;

    let file =
        File::create(index_path).map_err(|err| eprintln!("Can not create index file : {err}"))?;
    let writer = BufWriter::new(file);

    println!("Saving {index_path}...");
    let model = model.lock().unwrap();
    serde_json::to_writer_pretty(writer, &*model)
        .map_err(|err| eprintln!("Can not make entry in index file : {err}"))?;
    println!("Saved {index_path}");
    Ok(())
}
