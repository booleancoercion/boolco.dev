use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{Encoding, SaltString};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use base64::Engine;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{query, SqlitePool};

const HASH_ENCODING: Encoding = Encoding::B64;
const TICKET_ENGINE: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

#[derive(Debug)]
pub struct Db {
    pool: SqlitePool,
    argon2: Argon2<'static>,
}

#[cfg(not(feature = "prepare_db"))]
impl Db {
    pub async fn new(filename: &str, pepper: &'static [u8]) -> Self {
        let options = SqliteConnectOptions::new()
            .filename(filename)
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();

        create_tables(&pool).await;

        query!(
            "\
INSERT OR IGNORE INTO visitors (id, visitors) VALUES (0, 0);

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_name ON users (name);
CREATE UNIQUE INDEX IF NOT EXISTS idx_tickets ON registration_tickets (ticket);
"
        )
        .execute(&pool)
        .await
        .unwrap();

        let argon2 = Argon2::new_with_secret(
            pepper,
            argon2::Algorithm::default(),
            argon2::Version::default(),
            argon2::Params::default(),
        )
        .unwrap();

        Db { pool, argon2 }
    }

    pub async fn verify_user(&self, username: &str, password: &str) -> bool {
        let Ok(hash) = query!("SELECT password_hash FROM users WHERE name = ?;", username)
            .fetch_one(&self.pool)
            .await
            .map(|x| x.password_hash)
        else {
            return false;
        };

        // hashes in the DB are expected to be valid
        let hash = PasswordHash::parse(&hash, HASH_ENCODING).unwrap();
        self.argon2
            .verify_password(password.as_bytes(), &hash)
            .is_ok()
    }

    pub async fn insert_user(&self, ticket: &str, username: &str, password: &str) -> bool {
        let salt = SaltString::generate(&mut OsRng);
        let hash = self
            .argon2
            .hash_password(password.as_bytes(), &salt)
            .unwrap()
            .to_string();

        query!(
            "\
BEGIN TRANSACTION;
DELETE FROM registration_tickets WHERE ticket = ?;
INSERT INTO users (name, password_hash) VALUES (?, ?);
COMMIT TRANSACTION;",
            ticket,
            username,
            hash
        )
        .execute(&self.pool)
        .await
        .is_ok()
    }

    pub async fn generate_registration_ticket(&self) -> String {
        // *theoretically*, there is an astronomically small chance of a collision here,
        // so we have a loop.
        loop {
            let mut bytes = [0u8; 128];
            OsRng.fill_bytes(&mut bytes);
            let ticket = TICKET_ENGINE.encode(bytes);

            if dbg!(
                query!(
                    "INSERT INTO registration_tickets (ticket) VALUES (?);",
                    ticket
                )
                .execute(&self.pool)
                .await
            )
            .is_ok()
            {
                break ticket;
            }
        }
    }
}

async fn create_tables(conn: impl sqlx::SqliteExecutor<'_>) {
    query!(
        "\
CREATE TABLE IF NOT EXISTS messages(
    id          INTEGER     PRIMARY KEY,
    name        TEXT        NOT NULL,
    content     TEXT        NOT NULL,
    timestamp   DATETIME    NOT NULL
);

CREATE TABLE IF NOT EXISTS visitors(
    id          INTEGER     PRIMARY KEY,
    visitors    INTEGER     NOT NULL
);

CREATE TABLE IF NOT EXISTS users(
    id              INTEGER PRIMARY KEY,
    name            TEXT    NOT NULL UNIQUE,
    password_hash   TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS user_permissions(
    user_id INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE ON UPDATE CASCADE,
    admin   BOOLEAN NOT NULL
);

CREATE TABLE IF NOT EXISTS registration_tickets(
    id      INTEGER PRIMARY KEY,
    ticket  TEXT    NOT NULL UNIQUE
);"
    )
    .execute(conn)
    .await
    .unwrap();
}

#[cfg(feature = "prepare_db")]
pub async fn prepare_db(filename: &str) {
    use sqlx::{Connection, SqliteConnection};

    let options = SqliteConnectOptions::new()
        .filename(filename)
        .create_if_missing(true);
    let mut conn = SqliteConnection::connect_with(&options).await.unwrap();

    create_tables(&mut conn).await;
}
