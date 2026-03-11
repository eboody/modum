pub mod error;
pub mod helpers;
pub mod http;
pub mod partials;
pub mod storage;
pub mod user;

use crate::app::user::Repository;
use crate::http::Client;
use crate::user::UserRepository;
pub use crate::app::user::Error;

mod app {
    pub mod user {
        pub struct Repository;
        pub struct Error;
    }
}

#[allow(dead_code)]
fn uses_flattened_import(repo: Repository) -> Repository {
    repo
}

#[allow(dead_code)]
fn uses_broader_flattened_imports(client: Client, repo: UserRepository) -> (Client, UserRepository) {
    (client, repo)
}
