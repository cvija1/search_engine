XHTML TF-IDF Search Engine

A simple search engine written for practicing Rust.

The program recursively indexes all .xhtml files in a folder using TF-IDF, then provides a web page where you can search for documents by words.

`Build`

cargo build --release

`Run`

First run

cargo run --release index 'path_to_folder'

Then


cargo run --release serve
