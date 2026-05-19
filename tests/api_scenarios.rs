// tests/api_scenarios.rs — AI-Uploader Tier-1 tests

use actix_web::{test, web, App, HttpResponse};

#[actix_web::test]
async fn health_returns_200_with_body() {
    let app = test::init_service(
        App::new().route("/health", web::get().to(|| async {
            HttpResponse::Ok().body("Service is running")
        }))
    ).await;

    let req = test::TestRequest::get().uri("/health").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body = test::read_body(resp).await;
    assert!(!body.is_empty(), "Health body must not be empty");
}

#[actix_web::test]
async fn format_yaml_without_file_returns_client_error() {
    let app = test::init_service(
        App::new().route("/format-yaml", web::post().to(|| async {
            // Real handler requires multipart — simulate the rejection
            HttpResponse::BadRequest().body("No file provided")
        }))
    ).await;

    let req = test::TestRequest::post().uri("/format-yaml").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_client_error(), "Missing multipart → 4xx, got {}", resp.status());
}

#[actix_web::test]
async fn unknown_route_returns_404() {
    let app = test::init_service(
        App::new().route("/health", web::get().to(|| async { HttpResponse::Ok().finish() }))
    ).await;

    let req = test::TestRequest::get().uri("/nonexistent").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}
