use std::{
    fs,
    net::{SocketAddr, UdpSocket},
    path::PathBuf,
};//standard
use mime_guess;
use tokio::{
    fs::File,
    io::{AsyncWriteExt, BufReader},   // BufReader added for streaming
    net::TcpListener,
};
use tokio_util::io::ReaderStream; //tokio
use axum::{
    body::Body,
    extract::{Multipart, Path, DefaultBodyLimit},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Json,
    Router,
    serve,
};//axum

fn get_local_ip() -> Option<String> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;//connects to the IP address of the user (this computer)
    sock.connect("8.8.8.8:80").ok()?;//gives a default connection I think
    Some(sock.local_addr().ok()?.ip().to_string())//strings the IP
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(index))
        .route("/upload", post(upload))
        .route("/files", get(list_files))
        .route("/download/{name}", get(download))
        .layer(DefaultBodyLimit::max(1024 * 1024 * 1024));//1024*1024*1024 is 1gb, so a 1gb limit is tied to any upload for the safety of the server

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = TcpListener::bind(addr).await.unwrap();

    let lan_ip = get_local_ip().unwrap_or_else(|| "unknown".into());
    println!(" rShare running:");
    println!("  Local  -> http://localhost:8080");
    println!("  LAN    -> http://{}:8080", lan_ip);//gives away the lan ID of the server

    serve(listener, app.into_make_service()).await.unwrap();
}

async fn index() -> Html<&'static str> {
    // A minimal HTML/JS front page
    //maybe I could make this a bit more pretty?
    Html(r#"
<!DOCTYPE html>
<html>
<head>
<title>Rust--Share</title>
<style>
body { font-family: sans-serif; max-width:600px; margin:40px auto; }
input[type=file] { margin:10px 0; }
</style>
</head>
<body>
<h1>Rust--Share File Host</h1>

<h3>Upload a file</h3>
<form id="upload-form" enctype="multipart/form-data" method="post" action="/upload">
  <input type="file" name="file" />
  <button type="submit">Upload</button>
</form>

<h3>Available Files</h3>
<ul id="file-list"></ul>

<script>
async function refreshFiles(){
  const res = await fetch('/files');
  const files = await res.json();
  const list = document.getElementById('file-list');
  list.innerHTML = '';
  files.forEach(name=>{
    const li = document.createElement('li');
    li.innerHTML = `<a href="/download/${name}">${name}</a>`;
    list.appendChild(li);
  });
}
refreshFiles();
</script>
</body>
</html>
"#)
}
//handles uploads from server -> device..?
async fn upload(mut multipart: Multipart) -> impl IntoResponse {
    while let Some(mut field) = multipart.next_field().await.unwrap() {
        if let Some(filename) = field.file_name().map(|s| s.to_string()) {
            let path = PathBuf::from("uploads").join(&filename);
            let mut file = File::create(&path).await.unwrap();

            let mut written: u64 = 0;
            const MAX_SIZE: u64 = 1024 * 1024 * 1024; // 1 GB limit (example)

            while let Some(chunk) = field.chunk().await.unwrap() {
                written += chunk.len() as u64;
                if written > MAX_SIZE {
                    // Stop if file is too large
                    return (
                        axum::http::StatusCode::PAYLOAD_TOO_LARGE,
                        "File too big",
                    )
                        .into_response();
                }
                file.write_all(&chunk).await.unwrap();
            }
        }
    }

    Redirect::to("/").into_response()
}
async fn list_files() -> Json<Vec<String>> {
    let mut names = vec![];
    if let Ok(entries) = fs::read_dir("uploads") {
        for e in entries.flatten() {
            if let Some(name) = e.file_name().to_str() {
                names.push(name.to_string());
            }
        }
    }
    Json(names)
}
//handles downloads device -> server
async fn download(Path(name): Path<String>) -> impl IntoResponse {
    let path = PathBuf::from("uploads").join(&name);

    if !path.exists() {
        return (StatusCode::NOT_FOUND, "File not found").into_response();
    }

    let file = match File::open(&path).await {
        Ok(f) => f,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Can't open file").into_response(),
    };

    let stream = ReaderStream::new(BufReader::new(file));
    let body = Body::from_stream(stream);

    // Guess MIME type (or fallback to binary)
    let mime = mime_guess::from_path(&path).first_or_octet_stream();

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        mime.to_string().parse().unwrap(),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}\"", name).parse().unwrap(),
    );

    (headers, body).into_response()
}