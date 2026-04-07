use core::f64;
use chrono;
use std::{
    env,
    fs,
    net::{SocketAddr, UdpSocket},
    path::PathBuf,
    io::{self, Write},
    
};//standard
use axum_server::tls_rustls::RustlsConfig;
use once_cell::sync::Lazy;
use mime_guess;
use tower_cookies::{Cookies, Cookie, CookieManagerLayer};
use tokio::{
    fs::File,
    io::{AsyncWriteExt, BufReader,BufWriter},   // BufReader added for streaming  
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

//Struct for the config file
pub struct AppConfig {
    pub max_upload_size: u64,
    pub upload_speed_bps: u64,   // 0 means unlimited
    pub download_speed_bps: u64, // 0 means unlimited
}

//Let's create a config for users. 
static CONFIG: Lazy<AppConfig> = Lazy::new(|| {
    let config_path = "config.ini";
    
    // Set your defaults here
    let mut current_config = AppConfig {
        max_upload_size: 1024 * 1024 * 1024, // 1GB
        upload_speed_bps: 1024*1024,                 // 1 MB default
        download_speed_bps: 1024*1024,               // 1 MB default
    };

    if !std::path::Path::new(config_path).exists() {
        println!("Config file not found. Creating {}...", config_path);
        let mut file = std::fs::File::create(config_path).expect("Failed to create config file");
        //rewrite this so that it can use multipliers.
        //now it should be able to parse itself...
        writeln!(file, "[Settings]").unwrap();
        writeln!(file, "# Set max upload size in bytes. 1024*1024*1024 = 1GB").unwrap();
        writeln!(file, "# default is 1024*1024*1024 bytes").unwrap();
        writeln!(file, "file_Size=1024*1024*1024").unwrap();
        writeln!(file, "# Set max upload/download speed in bytes per second (0 = unlimited), 1024*1024 = 1MB Default").unwrap();
        writeln!(file, "upload_speed=1024*1024").unwrap();
        writeln!(file, "download_speed=1024*1024").unwrap();
        
        return current_config;
    }
    //I want users to be able to input multipliers.
    //this parses strings and returns the bit size for the program.

    // Read the file and update the struct if values are found
    let content = std::fs::read_to_string(config_path).unwrap_or_default();
    //for all the lines...
    for line in content.lines() {
        // Ignore lines that start with '#' (comments)
        if line.trim().starts_with('#') {
            continue; 
        }

        if let Some(val) = line.strip_prefix("file_Size=") {
            current_config.max_upload_size = parse_math_string(val, current_config.max_upload_size);
            
        } else if let Some(val) = line.strip_prefix("upload_speed=") {
            current_config.upload_speed_bps = parse_math_string(val, current_config.upload_speed_bps);
            
        } else if let Some(val) = line.strip_prefix("download_speed=") {
            current_config.download_speed_bps = parse_math_string(val, current_config.download_speed_bps);
        }
    }
    
    current_config
});
    ///This function parses the math strings found in the config
    /// 
    /// * `input` - the input string from the config file
    /// * `default_size` - The default size made by me. it can be found in the AppConfig as a default.
    /// * `parsed_anything` - The boolean that asks if the program could actually read the string from the user
    /// * `total` - The u64 value returned if the function works and returns a proper value.
  fn parse_math_string(input: &str, default_size: u64) -> u64 {
    let mut total: u64 = 1;
    let mut parsed_anything = false;

    // Split the string by the asterisk
    for part in input.split('*') {
        let clean_part = part.trim();
        
        // Skip empty parts (e.g., if someone typed "1024 * ")
        if clean_part.is_empty() {
            continue;
        }

        // Try to parse the chunk into a number
        match clean_part.parse::<u64>() {
            Ok(num) => {
                // saturating_mul prevents the server from crashing if a user 
                // types a number so big it overflows Rust's u64 limit!
                total = total.saturating_mul(num);
                parsed_anything = true;
            }
            Err(_) => {
                // If they typed letters like "1024 * apples", give up and return default
                println!("Warning: Invalid math in config. Falling back to default.");
                return default_size;
            }
        }
    }

    if parsed_anything {
        total
    } else {
        default_size
    }
}


///Ensures certificates for HTTPS by looking for certificates, and creating them if they don't exist.
///  - This is necessary for initialization
/// 
/// * `cert_path` - The path of the certificates
/// * `key_path` - The path of the key
/// * `pem_serialized` - the serialied cerificate
/// * `key_serialized` - the serialied key
fn ensure_certificates() -> Result<(), Box<dyn std::error::Error>> {
    let cert_path = PathBuf::from("cert.pem");
    let key_path = PathBuf::from("key.pem");

    if cert_path.exists() && key_path.exists() {
        return Ok(());
    }
    //feedback
    println!("Generating self-signed certificates...");

    // Generate a certificate for "localhost" and the local IP
    let mut params = rcgen::CertificateParams::new(vec!["localhost".to_string(), "127.0.0.1".to_string()]);
    
    // Attempt to add the actual LAN IP to the cert SANs (Subject Alternative Names)
    if let Some(ip) = get_local_ip() {
        params.subject_alt_names.push(rcgen::SanType::IpAddress(ip.parse()?));
    }
    //define and write certificates
    let cert = rcgen::Certificate::from_params(params)?;

    //should these be put into a struct? will that save on memory?
    let pem_serialized = cert.serialize_pem()?;
    let key_serialized = cert.serialize_private_key_pem();

    fs::write(&cert_path, pem_serialized)?;
    fs::write(&key_path, key_serialized)?;

    println!("Certificates generated successfully!");
    Ok(())
}

///Ensures the password file exists, creating it if not
/// 
/// * `env_path` - the path of the PASSWORD.env file
/// * `new_password` - The new password string
/// * `content` - represents the new_password in a format readily written to a fresh PASSWORD.env file
fn ensure_password() -> Result<(), Box<dyn std::error::Error>> {
    //ensure the passwords are there.
    let env_path = PathBuf::from("PASSWORD.env");

    if env_path.exists() {
        return Ok(());
    }

    println!("!--------------------------------------------------!");
    println!("First time setup: No password found.");
    println!("Please enter a password for rShare: ");
    println!("?--------------------------------------------------?\n");
    io::stdout().flush()?; // Ensure the prompt prints immediately

    let mut new_password = String::new();
    io::stdin().read_line(&mut new_password)?;
    let new_password = new_password.trim(); // Remove the newline character (needed because of the enter button pressed.)

    //cheeky error message
    if new_password.is_empty() {
        return Err("Password cannot be empty. You don't want that.".into());
    }

    // Save the new password to file
    let content = format!("APP_PASSWORD={}", new_password);
    fs::write(&env_path, content)?;

    println!("Password saved to 'PASSWORD.env'.");
    println!("~--------------------------------------------------~\n");
    
    // load the env file immediately.
    dotenvy::from_filename("PASSWORD.env").ok();

    Ok(())
}



use serde::Deserialize;
///Grabs the local IP to bind the socket
/// 
/// * `sock` - the IP address of the computer
/// * `Option<String>` - the string of "sock"
fn get_local_ip() -> Option<String> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;//connects to the IP address of the user (this computer)
    sock.connect("8.8.8.8:80").ok()?;
    Some(sock.local_addr().ok()?.ip().to_string())//strings the IP
}

///Returns the current time of user
/// 
/// * `time` - The local time of the user in string
/// Changed to only grab the first 19 Characters in order to stop the time stamp from being too accurate and annoying.
fn get_time() -> String{
    let mut time = chrono::offset::Local::now().to_string();
    time.truncate(19);
    return time;
}


#[tokio::main]
async fn main() {
    
    //1. Create uploads folder immediately (could go later but nahhh)
    std::fs::create_dir_all("uploads").expect("Failed to create uploads folder");
    
    // 2. Ensure certificates exist before starting the router.
    if let Err(e) = ensure_certificates() {
        eprintln!("Error generating certificates: {}", e);
        return;
    }
    //3. Make sure that there is a browser password before starting the router, too.

    if let Err(e) = ensure_password() {
        eprintln!("Error setting password: {}", e);
        return;
    }

    //define the routes that the "website" allows
    let protected_routes = Router::new()
        .route("/", get(index)) //the main dashboard
        .route("/upload", post(upload)) //the "website" the browser is in during the upload..?
        .route("/files", get(list_files)) //the files
        .route("/download/{name}", get(download)) 

        //1024*1024*1024 is 1gb, so change to what you want, if it's safe to do so. This changes the max size the user can upload to hostPC
        .layer(DefaultBodyLimit::max(CONFIG.max_upload_size as usize))
        
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
    println!("  !Note: Accept the browser warning to proceed, connection is secure!");

    //make some pretty values for the user.

    let pretty_max_size = CONFIG.max_upload_size as f64 / (1024.0*1024.0*1024.0);
    let pretty_upload_speed =CONFIG.upload_speed_bps as f64 / (1024.0*1024.0);
    let pretty_download_speed =CONFIG.upload_speed_bps as f64 / (1024.0*1024.0);

    println!("\n   Upload Speed : {:.2}MB/s || Download Speed : {:.2}MB/s",pretty_upload_speed,pretty_download_speed);
    println!("-~ Max File Size Set To {:.2} GB | This Can Be Changed In Config.ini ~-\n",pretty_max_size);

    //give a special message if upload or download are maximum.
    if pretty_upload_speed ==0.0{
    println!("!~`Upload Speed is set to Maximum`~!");
    }if pretty_download_speed ==0.0{
    println!("!~`Download Speed is set to Maximum`~!");
    }
    println!("~-------------------------------------------------------------------------------------~");
    // 2. Bind using axum-server with the TLS config
    axum_server::bind_rustls(addr, config)
        .serve(app.into_make_service())
        .await
        .unwrap();
}


///Call the app password file, called PASSWORD.env
static APP_PASSWORD: Lazy<String> = Lazy::new(|| {
    dotenvy::from_filename("PASSWORD.env").ok(); // load file
    env::var("APP_PASSWORD").expect("APP_PASSWORD not set")
});


///calls for the index.html
async fn index() -> Html<&'static str> {
    Html(include_str!("../index.html"))
}

///this is a struct for the password.
#[derive(Deserialize)]
struct LoginForm { password: String }

///calls the login.html
async fn login_form() -> Html<&'static str> {
    Html(include_str!("../login.html"))
}

///handles the login function of the software
/// 
/// * `cookies` - The cookies for the session
/// * Returns feedback for correct and incorrect logins.
/// * `data.password` - the password returned from the login.html
async fn login_submit(
    cookies: tower_cookies::Cookies,
    Form(data): Form<LoginForm>,
) -> Redirect {
    
    if data.password == *APP_PASSWORD {
    cookies.add(Cookie::new("auth", "ok"));
    //------------------------------------------------------------------------------------------------------
    println!("Connected User to dashboard on {}",get_time());
    
    Redirect::to("/")
    }
    //try to incorporate an Anti Bruteforce technique?
else {
    
    println!("Password incorrect.");
    
    Redirect::to("/login")  //go back to the login :)
}}

///accept or reject users based on login or cookies.
/// 
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
    println!("System denied forceful entry on {}",get_time());
    Err(StatusCode::UNAUTHORIZED)
}}


///handles uploads from server to device
/// 
/// * `file` - new user file
/// * `path` - the path to the new upload. located in the uploads folder.
/// * `chunk_size` - the speed from config file
/// 
async fn upload(mut multipart: Multipart) -> impl IntoResponse {
    while let Some(mut field) = multipart.next_field().await.unwrap() {
        if let Some(filename) = field.file_name().map(|s| s.to_string()) {

            let file_name= field.file_name().map(|s| s.to_string());
            let name_of_file = file_name.unwrap();
            
            let path = PathBuf::from("uploads").join(&filename);
            
            let file = File::create(&path).await.unwrap();
            let chunk_size = CONFIG.upload_speed_bps as usize;
            let mut buf_writer = BufWriter::with_capacity(chunk_size, file);
            
            let mut written: u64 = 0;
            println!("\nBeginning Upload Now...");
            // 3. Process the incoming network chunks
            while let Some(chunk) = field.chunk().await.unwrap() {
                written += chunk.len() as u64;
                //added a progress tracker here.
            use std::io::Write; // Required for the flush() command below
                print!("\rUploading '{}' || {} Megabytes Written",name_of_file,written/(1024*1024));
            //update the terminal immediately.
            std::io::stdout().flush().unwrap(); 

                if written > CONFIG.max_upload_size {
                    return (
                        axum::http::StatusCode::PAYLOAD_TOO_LARGE,
                        "File too big",
                    )
                        .into_response();
                }
                // -- APPLYING THE UPLOAD SPEED LIMIT --
                if CONFIG.upload_speed_bps > 0 {
                // Calculate how many seconds this specific chunk *should* take to process
                let seconds_for_chunk = chunk.len() as f64 / CONFIG.upload_speed_bps as f64;
                let sleep_duration = std::time::Duration::from_secs_f64(seconds_for_chunk);
        
                // Force the server to pause, effectively throttling the upload
                tokio::time::sleep(sleep_duration).await;
            }
                // Write the network chunk into our RAM buffer. 
                // It will automatically flush to disk when the 1MB limit is hit.
                buf_writer.write_all(&chunk).await.unwrap();
            }
            
            // 4. IMPORTANT: Flush the writer!
            // When the upload finishes, there might be a partially filled buffer 
            // (e.g., 500KB) still sitting in RAM. This forces it to write to the disk.
            buf_writer.flush().await.unwrap();
            //added some pretty diagnostic stuff.
            println!("\nUser uploaded '{}' to the dashboard on {}",name_of_file,get_time());
        }
    }
    Redirect::to("/").into_response()
}

///list the files
/// 
/// * `names` - the string of the names of the files in the upload folder.
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
//high level code...
async fn download(Path(name): Path<String>) -> impl IntoResponse {
    let path = PathBuf::from("uploads").join(&name);
    //the file must exist.
    if !path.exists() {
        println!("ERROR! Path does not exist!\n");
        return (StatusCode::NOT_FOUND, "File not found").into_response();
    }
    //the file must be accessible.
    let file = match File::open(&path).await {
        Ok(f) => f,
        Err(_) => {println!("ERROR! File not Accessible!\n");
        return (StatusCode::INTERNAL_SERVER_ERROR, "Can't open file").into_response()}, 
    };


    /*-- TAKING CONTROL OF CHUNKING --
    64 * 1024 = 65,536 bytes (64 KB). 
    You can increase this (e.g., 1024 * 1024 for 1MB chunks) to speed up LAN transfers
    at the cost of slightly higher RAM usage per active download.
    we live in a modern era, so we can have modern RAM usage lol. 
    Maybe I should add a config for this too??? Is that even.. necessary? imagine I set it to 1MB though
    That's already a HUGE performance leap. I have to speed test this.
    Maybe I should add a config file after all. 
    Using Lazy should provide the easiest method for introducing it,
     I could modify my config.ini to have a download speed config.

*/


    //splitting the file up.
    let chunk_size = CONFIG.download_speed_bps as usize; //now controlled properly
    let buf_reader = BufReader::with_capacity(chunk_size, file);
    

    //have the stream adapt to the values it is given.
    let stream = ReaderStream::new(buf_reader);
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
    //give the terminal some feedback for downloads
    println!("User downloaded '{}' from the dashboard on {}",name,get_time());
    (headers, body).into_response()
    
}