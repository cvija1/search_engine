use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::{self, BufReader, BufWriter},
    path::{Path, PathBuf},
    process::{self, ExitCode},
};
use tiny_http::{Header, Method, Request, Response, Server};
use xml::reader::{EventReader, XmlEvent::Characters};
mod lexer;
use lexer::Lexer;
type TF = HashMap<String, usize>;
type DocFreq = HashMap<String, usize>;
type TFIndex = HashMap<PathBuf, (usize, TF)>;

#[derive(Default, Deserialize, Serialize)]
struct Model {
    df: DocFreq,
    tfi: TFIndex,
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
    eprintln!("Usage: {program} [SUBCOMMAND] [OPTIONS]");
    eprintln!("Subcommands:");
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

fn tf(t: &str, n: usize, d: &TF) -> f32 {
    *d.get(t).unwrap_or(&0) as f32 / n as f32
}

fn idf(t: &str, n: usize, d: &DocFreq) -> f32 {
    let n = n as f32;
    let count = *d.get(t).unwrap_or(&0) as f32;

    ((n + 1.0) / (count + 1.0)).log10() + 1.0
}

fn serve_request(model: &Model, mut request: Request) -> Result<(), ()> {
    println!(
        "receiver request method: {:?}, url:{:?}",
        request.method(),
        request.url(),
    );
    match (request.method(), request.url()) {
        (Method::Post, "/api/search") => {
            let mut buf = Vec::new();
            let _ = request.as_reader().read_to_end(&mut buf);
            let body = str::from_utf8(&buf)
                .map_err(|err| eprintln!("Can not get body : {err}"))?
                .chars()
                .collect::<Vec<_>>();
            let mut result = Vec::<(&Path, f32)>::new();
            for (path, (n, tf_table)) in &model.tfi {
                let mut total_tf_idf: f32 = 0.0;
                for token in Lexer::new(&body) {
                    total_tf_idf +=
                        tf(&token, *n, tf_table) * idf(&token, model.tfi.len(), &model.df);
                }
                result.push((path, total_tf_idf));
            }

            result.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            let response_string = result
                .iter()
                .take(10)
                .map(|res| res.0.to_string_lossy())
                .collect::<Vec<_>>()
                .join("\n");

            request
                .respond(Response::from_string(response_string))
                .map_err(|err| eprintln!("Can not make response : {err}"))?;
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

fn entry(model: &Model) -> Result<(), ()> {
    let mut args = env::args();
    let program = args.next().expect("path to program is provided");

    let subcommand = args.next().ok_or_else(|| {
        usage(&program);
        eprintln!("ERROR: no subcommand is provided");
    })?;

    match subcommand.as_str() {
        "serve" => {
            let address = args.next().unwrap_or("127.0.0.1:6969".to_string());
            let server = Server::http(&address)
                .map_err(|e| eprintln!("Error: could not start http server at {address} : {e}"))?;

            println!("Listening at {address}");
            for request in server.incoming_requests() {
                let _ = serve_request(model, request);
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
    let model: Model = index();
    match entry(&model) {
        Ok(()) => ExitCode::SUCCESS,
        Err(()) => ExitCode::FAILURE,
    }
}
fn index() -> Model {
    let index_path = "index.json";
    let index_file = File::open(index_path);
    if let Ok(index_file) = index_file {
        let read_buf = BufReader::new(index_file);
        serde_json::from_reader(read_buf).expect("Index file can not be read")
    } else {
        match create_index() {
            Ok(index) => index,
            Err(e) => {
                eprintln!("{e}");
                process::exit(1)
            }
        }
    }
}

fn index_folder(model: &mut Model, dir_path: &Path) -> Result<(), io::Error> {
    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let file_path = entry.path();
        if file_path.is_dir() {
            index_folder(model, &file_path)?;
        } else {
            if file_path.extension().is_some_and(|ext| ext == "xhtml") {
                println!("Indexing {file_path:?}");
                let content = read_entire_xml_file(&file_path)?
                    .chars()
                    .collect::<Vec<_>>();
                let mut tf = TF::new();

                let mut count = 0;
                for token in Lexer::new(&content) {
                    *tf.entry(token).or_insert(0) += 1;
                    count += 1;
                }
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

                model.tfi.insert(file_path, (count, tf));
            }
        }
    }
    Ok(())
}

fn create_index() -> Result<Model, io::Error> {
    let mut model: Model = Default::default();
    let index_path = "index.json";
    println!("Saving {index_path}...");
    let dir_path = Path::new(r"C:\rust_projekti\search_engine\docs.gl");

    let _ = index_folder(&mut model, dir_path);
    let file = File::create(index_path)?;
    let writer = BufWriter::new(file);

    serde_json::to_writer_pretty(writer, &model)?;
    let index_file = File::open(index_path);
    if let Ok(index_file) = index_file {
        let read_buf = BufReader::new(index_file);
        serde_json::from_reader(read_buf).map_err(|err| {
            eprintln!("{err}");
            io::Error::new(io::ErrorKind::Other, "Error:")
        })
    } else {
        Err(io::Error::new(io::ErrorKind::Other, "Error"))
    }
}
