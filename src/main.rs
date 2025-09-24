use std::{
    env,
    fs,
    net::{SocketAddr, UdpSocket},
    path::PathBuf,
};//standard
use once_cell::sync::Lazy;
use mime_guess;
use tower_cookies::{Cookies, Cookie, CookieManagerLayer};
use tokio::{
    fs::File,
    io::{AsyncWriteExt, BufReader},   // BufReader added for streaming
    net::TcpListener,
};
use dotenvy;
use tokio_util::io::ReaderStream; //tokio
use axum::{
    body::Body,
    extract::{Multipart, Path, DefaultBodyLimit},
    http::{Request,header, HeaderMap, StatusCode},
    middleware::Next,
    middleware,
    response::{Html, IntoResponse, Redirect,Response},
    routing::{get, post},
    Json,
    Router,
    serve,
    Form
};//axum
use serde::Deserialize;
fn get_local_ip() -> Option<String> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;//connects to the IP address of the user (this computer)
    sock.connect("8.8.8.8:80").ok()?;//gives a default connection I think
    Some(sock.local_addr().ok()?.ip().to_string())//strings the IP
}

#[tokio::main]
async fn main() {
    let protected_routes = Router::new()
        .route("/", get(index))
        .route("/upload", post(upload))
        .route("/files", get(list_files))
        .route("/download/{name}", get(download))
        .layer(DefaultBodyLimit::max(1024 * 1024 * 1024))//1024*1024*1024 is 1gb, so change to what you want, if it's safe to do so.
        .route_layer(middleware::from_fn(require_auth));

    let app = Router::new()
    .route("/login", get(login_form).post(login_submit))
    .merge(protected_routes)
    .layer(CookieManagerLayer::new());

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = TcpListener::bind(addr).await.unwrap();

    let lan_ip = get_local_ip().unwrap_or_else(|| "unknown".into());
    println!(" rShare running:");
    println!("  Local  -> http://localhost:8080/login");
    println!("  LAN    -> http://{}:8080/login", lan_ip);//gives away the lan ID of the server

    serve(listener, app.into_make_service()).await.unwrap();
}

static APP_PASSWORD: Lazy<String> = Lazy::new(|| {
    dotenvy::from_filename("PASSWORD.env").ok(); // load file
    env::var("APP_PASSWORD").expect("APP_PASSWORD not set")
});

async fn index() -> Html<&'static str> {
    // A minimal HTML/JS front page
    //maybe I could make this a bit more pretty?
    Html(r#"
<!DOCTYPE html>
<html>
<head>
<title>Rust--Share</title>
<style>
body {
    font-family: Arial, sans-serif;
    background: #f7f7f7;
    max-width: 700px;
    margin: 40px auto;
    padding: 20px;
    border-radius: 8px;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.14);
}
h1 { text-align: center; color: #333; }
p  { text-align: center; color: #333; }
button {
    padding: 6px 12px;
    background: #be3e1eff;
    color: white;
    border: none;
    border-radius: 4px;
    cursor: pointer;
}
button:hover { background: #45a049; }
ul { list-style: none; padding-left: 0; }
li { margin: 5px 0; }
</style>
</head>
<body>
<p>Welcome to Rust Share! Upload files below and share them across your network (almost) instantly.</p>

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

#[derive(Deserialize)]
struct LoginForm { password: String }

async fn login_form() -> Html<&'static str> {
    Html(r#"
    <style>
body {
    font-family: Arial, sans-serif;
    background: #f7f7f7;
    max-width: 700px;
    margin: 40px auto;
    padding: 20px;
    border-radius: 8px;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.48);
}
h1 { text-align: center; color: #333; }
p  { text-align: center; color: #333; }
button {
    padding: 6px 12px;
    background: #be3e1eff;
    color: white;
    border: none;
    border-radius: 4px;
    cursor: pointer;
}
button:hover { background: #45a049; }
ul { list-style: none; padding-left: 0; }
li { margin: 5px 0; }
</style>
    <h2>Enter Password:</h2>
    <form method="post" action="/login">
      <input type="password" name="password" placeholder="Password">
      <button type="submit">Login</button>
    </form>
    "#)
}

async fn login_submit(
    cookies: tower_cookies::Cookies,
    Form(data): Form<LoginForm>,
) -> Redirect {
    if data.password == *APP_PASSWORD {
    cookies.add(Cookie::new("auth", "ok"));
    Redirect::to("/")
} else {
    Redirect::to("/login")
}}
async fn require_auth(
    cookies: Cookies,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // take the cookie first so it lives long enough
if cookies
    .get("auth")
    .map(|c| c.value() == "ok")
    .unwrap_or(false)
{
    Ok(next.run(req).await)
} else {
    Err(StatusCode::UNAUTHORIZED)
}}
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