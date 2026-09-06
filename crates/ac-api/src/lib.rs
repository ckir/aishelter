//! HTTP error handling and response types for the Agent Commons API. 
 
//! This crate provides the `ApiResponse<T>` wrapper for consistent JSON 
//! responses and the `app_error_to_response` helper that maps domain-level 
//! `AcError` variants to appropriate HTTP status codes. 
 
/// Conversion of domain errors to HTTP responses. 
pub mod error; 
/// Standardised API response wrapper. 
pub mod types; 
/// Route definitions placeholder. 
pub mod routes;
