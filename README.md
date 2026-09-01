XHTML AND PDF TF-IDF Search Engine

A simple search engine written for practicing Rust.

The program recursively indexes all .xhtml and .pdf files in a folder using TF-IDF, then provides a web page where you can search for documents by words. It is multithread implementation where background thread indexing while we serve web page.

`Build`

cargo build --release

`Run`

cargo run --release serve 'path_to_folder'
