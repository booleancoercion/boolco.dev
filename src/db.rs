use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{Encoding, SaltString};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use base64::Engine;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{query, QueryBuilder, SqlitePool};

#[cfg(not(feature = "prepare_db"))]
use crate::game::GameMessage;

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
CREATE UNIQUE INDEX IF NOT EXISTS idx_tickets_name ON registration_tickets (name);
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

    pub async fn verify_user(&self, username: &str, password: &str) -> Option<i64> {
        let Ok(rec) = query!(
            "SELECT id, password_hash FROM users WHERE name = ?;",
            username
        )
        .fetch_one(&self.pool)
        .await
        else {
            return None;
        };

        // hashes in the DB are expected to be valid
        let hash = PasswordHash::parse(&rec.password_hash, HASH_ENCODING).unwrap();
        if self
            .argon2
            .verify_password(password.as_bytes(), &hash)
            .is_ok()
        {
            Some(rec.id)
        } else {
            None
        }
    }

    pub async fn register_user(&self, ticket: &str, password: &str) -> Option<i64> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = self
            .argon2
            .hash_password(password.as_bytes(), &salt)
            .unwrap()
            .to_string();

        let mut transaction = self.pool.begin().await.unwrap();

        let username = if let Ok(rec) = query!(
            "DELETE FROM registration_tickets WHERE ticket = ? RETURNING name;",
            ticket
        )
        .fetch_one(&mut *transaction)
        .await
        {
            rec.name
        } else {
            return None; // rolls back the transaction
        };

        match query!(
            "INSERT INTO users (name, password_hash) VALUES (?, ?) RETURNING id;",
            username,
            hash
        )
        .fetch_one(&mut *transaction)
        .await
        {
            Ok(rec) => {
                transaction.commit().await.unwrap();
                Some(rec.id)
            }
            Err(_) => None, // rolls back the transaction
        }
    }

    pub async fn generate_registration_ticket(&self, name: &str) -> Option<String> {
        let mut transaction = self.pool.begin().await.unwrap();

        if sqlx::query_scalar::<_, i64>(
            "SELECT EXISTS(SELECT 1 FROM registration_tickets WHERE name = ?);",
        )
        .bind(name)
        .fetch_one(&mut *transaction)
        .await
        .unwrap()
            == 1
        {
            return None;
        }

        if sqlx::query_scalar::<_, i64>("SELECT EXISTS(SELECT 1 FROM users WHERE name = ?);")
            .bind(name)
            .fetch_one(&mut *transaction)
            .await
            .unwrap()
            == 1
        {
            return None;
        }

        // *theoretically*, there is an astronomically small chance of a collision here,
        // so we have a loop.
        loop {
            let mut bytes = [0u8; 128];
            OsRng.fill_bytes(&mut bytes);
            let ticket = TICKET_ENGINE.encode(bytes);

            if query!(
                "INSERT INTO registration_tickets (name, ticket) VALUES (?, ?);",
                name,
                ticket
            )
            .execute(&mut *transaction)
            .await
            .is_ok()
            {
                transaction.commit().await.unwrap();
                break Some(ticket);
            }
        }
    }

    pub async fn get_username(&self, id: i64) -> Option<String> {
        query!("SELECT name FROM users WHERE id = ?;", id)
            .fetch_optional(&self.pool)
            .await
            .unwrap()
            .map(|x| x.name)
    }

    pub async fn get_visitors(&self) -> i64 {
        query!("SELECT visitors FROM visitors;")
            .fetch_one(&self.pool)
            .await
            .unwrap()
            .visitors
    }

    pub async fn set_visitors(&self, visitors: i64) {
        query!("UPDATE visitors SET visitors = ?;", visitors)
            .execute(&self.pool)
            .await
            .unwrap();
    }

    pub async fn get_messages(&self) -> Vec<GameMessage> {
        let messages = query!("SELECT * FROM messages ORDER BY id ASC;")
            .fetch_all(&self.pool)
            .await
            .unwrap();

        messages
            .into_iter()
            .map(|rec| GameMessage {
                name: rec.name,
                content: rec.content,
                ip: rec.ip.parse().unwrap(),
            })
            .collect()
    }

    pub async fn set_messages(&self, messages: &[GameMessage]) {
        query!("DELETE FROM messages;")
            .execute(&self.pool)
            .await
            .unwrap();

        if !messages.is_empty() {
            let mut query_builder: QueryBuilder<sqlx::Sqlite> =
                QueryBuilder::new("INSERT INTO messages(name, content, ip) ");
            query_builder.push_values(messages, |mut b, message| {
                b.push_bind(&message.name)
                    .push_bind(&message.content)
                    .push_bind(message.ip.to_string());
            });

            query_builder.build().execute(&self.pool).await.unwrap();
        }
    }
}

async fn create_tables(conn: impl sqlx::SqliteExecutor<'_>) {
    query!(
        "\
CREATE TABLE IF NOT EXISTS messages(
    id          INTEGER     NOT NULL PRIMARY KEY AUTOINCREMENT,
    name        TEXT        NOT NULL,
    content     TEXT        NOT NULL,
    ip          TEXT        NOT NULL
);

CREATE TABLE IF NOT EXISTS visitors(
    id          INTEGER     NOT NULL PRIMARY KEY,
    visitors    INTEGER     NOT NULL
);

CREATE TABLE IF NOT EXISTS users(
    id              INTEGER NOT NULL PRIMARY KEY,
    name            TEXT    NOT NULL UNIQUE,
    password_hash   TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS user_permissions(
    user_id INTEGER NOT NULL PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE ON UPDATE CASCADE,
    admin   BOOLEAN NOT NULL
);

CREATE TABLE IF NOT EXISTS registration_tickets(
    id      INTEGER NOT NULL PRIMARY KEY,
    name    TEXT    NOT NULl UNIQUE,
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
