use std::net::IpAddr;

use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{Encoding, SaltString};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use base64::Engine;
use rand::distributions::{Alphanumeric, DistString};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{query, query_as, QueryBuilder, SqlitePool};

#[cfg(not(feature = "prepare_db"))]
use crate::game::GameMessage;

const HASH_ENCODING: Encoding = Encoding::B64;
const TICKET_ENGINE: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

#[derive(Clone, Copy, Debug, Default)]
pub struct UserPermissions {
    inner: u64,
}

impl UserPermissions {
    const ADMIN: u8 = 0;
    const SHORT: u8 = 1;

    fn admin(&mut self, set: bool) {
        if set {
            self.inner |= 1u64 << Self::ADMIN;
        } else {
            self.inner &= !(1u64 << Self::ADMIN);
        }
    }

    pub fn is_admin(&self) -> bool {
        self.inner & (1u64 << Self::ADMIN) != 0
    }

    fn short(&mut self, set: bool) {
        if set {
            self.inner |= 1u64 << Self::SHORT;
        } else {
            self.inner &= !(1u64 << Self::SHORT);
        }
    }

    pub fn is_short(&self) -> bool {
        self.is_admin() || self.inner & (1u64 << Self::SHORT) != 0
    }
}

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
            .create_if_missing(true)
            .pragma("foreign_keys", "ON");
        let pool = SqlitePool::connect_with(options).await.unwrap();

        create_tables(&pool).await;

        query!(
            "\
INSERT OR IGNORE INTO visitors (id, visitors) VALUES (0, 0);

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_name    ON users                (name);
CREATE UNIQUE INDEX IF NOT EXISTS idx_tickets       ON registration_tickets (ticket);
CREATE UNIQUE INDEX IF NOT EXISTS idx_tickets_name  ON registration_tickets (name);
CREATE UNIQUE INDEX IF NOT EXISTS idx_shorts        ON short_links          (short);
CREATE        INDEX IF NOT EXISTS idx_shorts_userid ON short_links          (user_id);
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

    pub async fn get_permissions(&self, id: i64) -> Option<UserPermissions> {
        query!("SELECT * FROM user_permissions WHERE user_id = ?;", id)
            .fetch_optional(&self.pool)
            .await
            .unwrap()
            .map(|rec| {
                let mut perms = UserPermissions::default();

                perms.admin(rec.admin);
                perms.short(rec.short);

                perms
            })
    }

    pub async fn create_short_link(
        &self,
        user_id: i64,
        link: &str,
        short: Option<&str>,
    ) -> Option<String> {
        let mut transaction = self.pool.begin().await.unwrap();

        if sqlx::query_scalar::<_, i64>("SELECT EXISTS(SELECT 1 FROM users WHERE id = ?);")
            .bind(user_id)
            .fetch_one(&mut *transaction)
            .await
            .unwrap()
            == 0
        {
            return None;
        }

        let short = if let Some(short) = short {
            let ret = query!(
                "INSERT INTO short_links (user_id, url, short) VALUES (?, ?, ?);",
                user_id,
                link,
                short
            )
            .execute(&mut *transaction)
            .await;

            if ret.is_ok() {
                short.to_owned()
            } else {
                return None;
            }
        } else {
            let mut short = String::new();
            loop {
                generate_short(&mut short);
                let res = query!(
                    "INSERT INTO short_links (user_id, url, short) VALUES (?, ?, ?)",
                    user_id,
                    link,
                    short
                )
                .execute(&mut *transaction)
                .await;

                if res.is_ok() {
                    break short;
                }
            }
        };

        transaction.commit().await.unwrap();
        Some(short)
    }

    pub async fn get_short_link(&self, link: &str, peer_addr: IpAddr) -> Option<String> {
        let mut transaction = self.pool.begin().await.unwrap();

        let rec = query!("SELECT * FROM short_links WHERE short = ?;", link)
            .fetch_optional(&mut *transaction)
            .await
            .unwrap()?;

        let peer_addr = peer_addr.to_string();
        query!(
            "INSERT INTO short_link_stats (link_id, peer_addr) VALUES (?, ?);",
            rec.id,
            peer_addr
        )
        .execute(&mut *transaction)
        .await
        .unwrap();

        transaction.commit().await.unwrap();
        Some(rec.url)
    }

    pub async fn get_links(&self, user_id: i64) -> Vec<crate::short::Link> {
        query_as!(
            crate::short::Link,
            "SELECT id, url, short FROM short_links WHERE user_id = ? ORDER BY id ASC;",
            user_id
        )
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }

    pub async fn delete_if_owns_short_link(&self, user_id: i64, short: &str) -> bool {
        query!(
            "DELETE FROM short_links WHERE user_id = ? AND short = ? RETURNING id;",
            user_id,
            short
        )
        .fetch_optional(&self.pool)
        .await
        .unwrap()
        .is_some()
    }
}

const SHORT_LINK_LENGTH: usize = 5;

fn generate_short(short: &mut String) {
    short.clear();
    let mut rng = rand::thread_rng();
    Alphanumeric.append_string(&mut rng, short, SHORT_LINK_LENGTH);
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
    admin   BOOLEAN NOT NULL,
    short   BOOLEAN NOT NULL
);

CREATE TABLE IF NOT EXISTS registration_tickets(
    id      INTEGER NOT NULL PRIMARY KEY,
    name    TEXT    NOT NULl UNIQUE,
    ticket  TEXT    NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS short_links(
    id      INTEGER NOT NULL PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE ON UPDATE CASCADE,
    url     TEXT    NOT NULL,
    short   TEXT    NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS short_link_stats(
    id          INTEGER NOT NULL PRIMARY KEY,
    link_id     INTEGER NOT NULL REFERENCES short_links(id) ON DELETE CASCADE ON UPDATE CASCADE,
    peer_addr   TEXT NOT NULL
);
"
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
