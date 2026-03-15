pub mod error;
pub mod helpers;
pub mod components {
    pub mod button {
        pub struct Button;
    }
}
pub mod http;
pub mod partials;
pub mod storage;
pub mod user;

pub use crate::app::user::Error;
use crate::app::user::Repository;
use crate::http::Client;
use crate::user::UserRepository;

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
fn uses_broader_flattened_imports(
    client: Client,
    repo: UserRepository,
) -> (Client, UserRepository) {
    (client, repo)
}
