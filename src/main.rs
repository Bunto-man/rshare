use std::{
    env,
    fs,
    net::{SocketAddr, UdpSocket},
    path::PathBuf,
    io::{self, Write}
};//standard
use axum_server::tls_rustls::RustlsConfig;
use once_cell::sync::Lazy;
use mime_guess;
use tower_cookies::{Cookies, Cookie, CookieManagerLayer};
use tokio::{
    fs::File,
    io::{AsyncWriteExt, BufReader},   // BufReader added for streaming  
};
use dotenvy;
use tokio_util::io::ReaderStream; 
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
    Form
};//axum

//Help generate keys to use HTTPS
fn ensure_certificates() -> Result<(), Box<dyn std::error::Error>> {
    let cert_path = PathBuf::from("cert.pem");
    let key_path = PathBuf::from("key.pem");

    if cert_path.exists() && key_path.exists() {
        return Ok(());
    }

    println!("Generating self-signed certificates...");

    // Generate a certificate for "localhost" and the local IP
    let mut params = rcgen::CertificateParams::new(vec!["localhost".to_string(), "127.0.0.1".to_string()]);
    
    // Attempt to add the actual LAN IP to the cert SANs (Subject Alternative Names)
    if let Some(ip) = get_local_ip() {
        params.subject_alt_names.push(rcgen::SanType::IpAddress(ip.parse()?));
    }

    let cert = rcgen::Certificate::from_params(params)?;
    
    let pem_serialized = cert.serialize_pem()?;
    let key_serialized = cert.serialize_private_key_pem();

    fs::write(&cert_path, pem_serialized)?;
    fs::write(&key_path, key_serialized)?;

    println!("Certificates generated successfully!");
    Ok(())
}

fn ensure_password() -> Result<(), Box<dyn std::error::Error>> {
    let env_path = PathBuf::from("PASSWORD.env");

    if env_path.exists() {
        return Ok(());
    }

    println!("--------------------------------------------------");
    println!("First time setup: No password found.");
    print!("Please enter a password for rShare: ");
    io::stdout().flush()?; // Ensure the prompt prints immediately

    let mut new_password = String::new();
    io::stdin().read_line(&mut new_password)?;
    let new_password = new_password.trim(); // Remove the newline character

    if new_password.is_empty() {
        return Err("Password cannot be empty!".into());
    }

    // Save to file
    let content = format!("APP_PASSWORD={}", new_password);
    fs::write(&env_path, content)?;

    println!("Password saved to 'PASSWORD.env'.");
    println!("--------------------------------------------------");
    
    // Crucial: We must load the .env file immediately so the current process sees it
    dotenvy::from_filename("PASSWORD.env").ok();

    Ok(())
}



use serde::Deserialize;
fn get_local_ip() -> Option<String> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;//connects to the IP address of the user (this computer)
    sock.connect("8.8.8.8:80").ok()?;//gives a default connection I think
    Some(sock.local_addr().ok()?.ip().to_string())//strings the IP
}



#[tokio::main]
async fn main() {

    // 1. Ensure certificates exist before starting
    if let Err(e) = ensure_certificates() {
        eprintln!("Error generating certificates: {}", e);
        return;
    }
    //2. Make sure that there is a browser password before running!

    if let Err(e) = ensure_password() {
        eprintln!("Error setting password: {}", e);
        return;
    }

    let protected_routes = Router::new()
        .route("/", get(index))
        .route("/upload", post(upload))
        .route("/files", get(list_files))
        .route("/download/{name}", get(download))
        .layer(DefaultBodyLimit::max(25024 * 1024 * 1024))//1024*1024*1024 is 1gb, so change to what you want, if it's safe to do so.
        .route_layer(middleware::from_fn(require_auth));

    let app = Router::new()
    .route("/login", get(login_form).post(login_submit))
    .merge(protected_routes)
    .layer(CookieManagerLayer::new());

        // 1. Load the certificate and private key
        // Ensure cert.pem and key.pem are GENERATED!
    let config = RustlsConfig::from_pem_file(
        PathBuf::from("cert.pem"), 
        PathBuf::from("key.pem")
    )
    .await
    .expect("Failed to load TLS certificates! Run the openssl command first.");

    let lan_ip = get_local_ip().unwrap_or_else(|| "unknown".into());
    let port = 8080;
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

   println!(" rShare running (HTTPS):");
    println!("  Local  -> https://localhost:{}/login", port);
    println!("  LAN    -> https://{}:{}/login", lan_ip, port);
    println!("  (Note: Accept the browser warning to proceed)");

    // 2. Bind using axum-server with the TLS config
    axum_server::bind_rustls(addr, config)
        .serve(app.into_make_service())
        .await
        .unwrap();
}


//Call the app password file, called PASSWORD.env
static APP_PASSWORD: Lazy<String> = Lazy::new(|| {
    dotenvy::from_filename("PASSWORD.env").ok(); // load file
    env::var("APP_PASSWORD").expect("APP_PASSWORD not set")
});


//The HTML Code. keep it minimal
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