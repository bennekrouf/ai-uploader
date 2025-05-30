use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use actix_web::{web, App, HttpResponse, HttpServer, Error};
use actix_multipart::{Multipart, Field};
use futures_util::stream::StreamExt;
use futures_util::TryStreamExt;
use tempfile::NamedTempFile;
use tracing::{debug, error, info};
use uuid::Uuid;

mod extract_yaml;
mod format_yaml_with_ollama;
mod load_prompt;
mod models;
mod yaml_validator;

use crate::format_yaml_with_ollama::format_yaml_with_ollama;

struct AppState {
    template_path: String,
    system_prompt_path: String,
    user_prompt_path: String,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    info!("Starting YAML formatter HTTP service");

    // Define file paths
    let template_file_path = "template.yaml";
    let system_prompt_path = "prompt/system_prompt.txt";
    let user_prompt_path = "prompt/user_prompt.txt";

    // Ensure the prompt directory exists
    if !Path::new("prompt").exists() {
        std::fs::create_dir("prompt").expect("Failed to create prompt directory");
        info!("Created prompt directory");
    }

    // Check if prompt files exist
    for (path, name) in [
        (system_prompt_path, "system prompt"),
        (user_prompt_path, "user prompt"),
        (template_file_path, "template file"),
    ] {
        if !Path::new(path).exists() {
            error!("{} file not found at {}", name, path);
            panic!("Missing {} file", name);
        }
    }

    let app_state = web::Data::new(AppState {
        template_path: template_file_path.to_string(),
        system_prompt_path: system_prompt_path.to_string(),
        user_prompt_path: user_prompt_path.to_string(),
    });

    // Start HTTP server
    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/format-yaml", web::post().to(format_yaml_handler))
            .route("/health", web::get().to(health_check))
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}

async fn health_check() -> HttpResponse {
    HttpResponse::Ok().body("Service is running")
}

async fn save_field(field: Field) -> Result<String, Error> {
    let content_disposition = field.content_disposition();
    let filename = content_disposition.get_filename().unwrap_or("upload.txt");
    let filepath = format!("/tmp/{}-{}", Uuid::new_v4(), sanitize_filename::sanitize(filename));
    
    debug!("Saving uploaded file to {}", filepath);
    
    let mut temp_file = std::fs::File::create(&filepath)?;
    let mut bytes = web::BytesMut::new();
    
    let mut field_stream = field;
    while let Some(chunk) = field_stream.next().await {
        let data = chunk?;
        bytes.extend_from_slice(&data);
        temp_file.write_all(&data)?;
    }
    
    temp_file.flush()?;
    Ok(filepath)
}

async fn format_yaml_handler(
    multipart: Multipart,
    app_state: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    let mut input_path = None;
    
    // Process the multipart form data
    let mut multipart_data = multipart;
    
    info!("Processing uploaded file");
    field_loop: while let Ok(Some(field)) = multipart_data.try_next().await {
        if field.name() == "file" {
            input_path = Some(save_field(field).await?);
            break field_loop;
        }
    }
    
    let input_file_path = input_path.ok_or_else(|| {
        error!("No file was uploaded");
        actix_web::error::ErrorBadRequest("No file was uploaded")
    })?;

    info!("Processing file: {}", input_file_path);
    
    // Process the uploaded file
    match format_yaml_with_ollama(
        &input_file_path,
        &app_state.template_path,
        &app_state.system_prompt_path,
        &app_state.user_prompt_path,
    )
    .await {
        Ok(formatted_yaml) => {
            // Prepare the response
            info!("Successfully formatted YAML");
            
            // Create a temporary file for the response
            let mut temp_file = NamedTempFile::new()?;
            temp_file.write_all(formatted_yaml.as_bytes())?;
            temp_file.flush()?;
            
            // Clean up the input file
            if let Err(e) = std::fs::remove_file(&input_file_path) {
                error!("Failed to remove temporary input file: {}", e);
            }
            
            // Return the formatted YAML
            Ok(HttpResponse::Ok()
                .content_type("application/yaml")
                .append_header((
                    "Content-Disposition",
                    "attachment; filename=\"formatted_output.yaml\"",
                ))
                .body(formatted_yaml))
        }
        Err(e) => {
            error!("Error formatting YAML: {}", e);
            // Clean up the input file
            if let Err(cleanup_err) = std::fs::remove_file(&input_file_path) {
                error!("Failed to remove temporary input file: {}", cleanup_err);
            }
            
            Ok(HttpResponse::InternalServerError().body(format!("Error: {}", e)))
        }
    }
}
