// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

use std::collections::HashMap;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::sync::Arc;

use serde::Deserialize;
use surrealdb::engine::any::Any;
use surrealdb::opt::auth::Database;
use surrealdb::Surreal;
use tokio::sync::OnceCell;

use crate::raw::adapters::kv;
use crate::raw::normalize_root;
use crate::raw::ConfigDeserializer;
use crate::Buffer;
use crate::Builder;
use crate::Capability;
use crate::Error;
use crate::ErrorKind;
use crate::Scheme;

/// Config for Surrealdb services support.
#[derive(Default, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct SurrealdbConfig {
    connection_string: Option<String>,
    username: Option<String>,
    password: Option<String>,
    namespace: Option<String>,
    database: Option<String>,
    table: Option<String>,
    key_field: Option<String>,
    value_field: Option<String>,
    root: Option<String>,
}

impl Debug for SurrealdbConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_struct("SurrealdbConfig");

        d.field("connection_string", &self.connection_string)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .field("namespace", &self.namespace)
            .field("database", &self.database)
            .field("table", &self.table)
            .field("key_field", &self.key_field)
            .field("value_field", &self.value_field)
            .field("root", &self.root)
            .finish()
    }
}
#[doc = include_str!("docs.md")]
#[derive(Default)]
pub struct SurrealdbBuilder {
    config: SurrealdbConfig,
}

impl Debug for SurrealdbBuilder {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurrealdbBuilder")
            .field("config", &self.config)
            .finish()
    }
}

impl SurrealdbBuilder {
    /// Set the connection_string of the surrealdb service.
    ///
    /// This connection string is used to connect to the surrealdb service. There are url based formats:
    ///
    /// ## Url
    ///
    /// - `ws://ip:port`
    /// - `wss://ip:port`
    /// - `http://ip:port`
    /// - `https://ip:port`
    pub fn connection_string(&mut self, connection_string: &str) -> &mut Self {
        if !connection_string.is_empty() {
            self.config.connection_string = Some(connection_string.to_string());
        }
        self
    }

    /// set the working directory, all operations will be performed under it.
    ///
    /// default: "/"
    pub fn root(&mut self, root: &str) -> &mut Self {
        if !root.is_empty() {
            self.config.root = Some(root.to_string());
        }
        self
    }

    /// Set the table name of the surrealdb service for read/write.
    pub fn table(&mut self, table: &str) -> &mut Self {
        if !table.is_empty() {
            self.config.table = Some(table.to_string());
        }
        self
    }

    /// Set the username of the surrealdb service for signin.
    pub fn username(&mut self, username: &str) -> &mut Self {
        if !username.is_empty() {
            self.config.username = Some(username.to_string());
        }
        self
    }

    /// Set the password of the surrealdb service for signin.
    pub fn password(&mut self, password: &str) -> &mut Self {
        if !password.is_empty() {
            self.config.password = Some(password.to_string());
        }
        self
    }

    /// Set the namespace of the surrealdb service for read/write.
    pub fn namespace(&mut self, namespace: &str) -> &mut Self {
        if !namespace.is_empty() {
            self.config.namespace = Some(namespace.to_string());
        }
        self
    }

    /// Set the database of the surrealdb service for read/write.
    pub fn database(&mut self, database: &str) -> &mut Self {
        if !database.is_empty() {
            self.config.database = Some(database.to_string());
        }
        self
    }

    /// Set the key field name of the surrealdb service for read/write.
    ///
    /// Default to `key` if not specified.
    pub fn key_field(&mut self, key_field: &str) -> &mut Self {
        if !key_field.is_empty() {
            self.config.key_field = Some(key_field.to_string());
        }
        self
    }

    /// Set the value field name of the surrealdb service for read/write.
    ///
    /// Default to `value` if not specified.
    pub fn value_field(&mut self, value_field: &str) -> &mut Self {
        if !value_field.is_empty() {
            self.config.value_field = Some(value_field.to_string());
        }
        self
    }
}

impl Builder for SurrealdbBuilder {
    const SCHEME: Scheme = Scheme::Surrealdb;
    type Accessor = SurrealdbBackend;

    fn from_map(map: HashMap<String, String>) -> Self {
        let config = SurrealdbConfig::deserialize(ConfigDeserializer::new(map))
            .expect("config deserialize must succeed");

        SurrealdbBuilder { config }
    }

    fn build(&mut self) -> crate::Result<Self::Accessor> {
        let connection_string = match self.config.connection_string.clone() {
            Some(v) => v,
            None => {
                return Err(
                    Error::new(ErrorKind::ConfigInvalid, "connection_string is empty")
                        .with_context("service", Scheme::Surrealdb),
                )
            }
        };

        let namespace = match self.config.namespace.clone() {
            Some(v) => v,
            None => {
                return Err(Error::new(ErrorKind::ConfigInvalid, "namespace is empty")
                    .with_context("service", Scheme::Surrealdb))
            }
        };
        let database = match self.config.database.clone() {
            Some(v) => v,
            None => {
                return Err(Error::new(ErrorKind::ConfigInvalid, "database is empty")
                    .with_context("service", Scheme::Surrealdb))
            }
        };
        let table = match self.config.table.clone() {
            Some(v) => v,
            None => {
                return Err(Error::new(ErrorKind::ConfigInvalid, "table is empty")
                    .with_context("service", Scheme::Surrealdb))
            }
        };

        let username = self.config.username.clone().unwrap_or_default();
        let password = self.config.password.clone().unwrap_or_default();
        let key_field = self
            .config
            .key_field
            .clone()
            .unwrap_or_else(|| "key".to_string());
        let value_field = self
            .config
            .value_field
            .clone()
            .unwrap_or_else(|| "value".to_string());
        let root = normalize_root(
            self.config
                .root
                .clone()
                .unwrap_or_else(|| "/".to_string())
                .as_str(),
        );

        Ok(SurrealdbBackend::new(Adapter {
            db: OnceCell::new(),
            connection_string,
            username,
            password,
            namespace,
            database,
            table,
            key_field,
            value_field,
        })
        .with_root(&root))
    }
}

/// Backend for Surrealdb service
pub type SurrealdbBackend = kv::Backend<Adapter>;

#[derive(Clone)]
pub struct Adapter {
    db: OnceCell<Arc<Surreal<Any>>>,
    connection_string: String,

    username: String,
    password: String,
    namespace: String,
    database: String,

    table: String,
    key_field: String,
    value_field: String,
}

impl Debug for Adapter {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Adapter")
            .field("connection_string", &self.connection_string)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .field("namespace", &self.namespace)
            .field("database", &self.database)
            .field("table", &self.table)
            .field("key_field", &self.key_field)
            .field("value_field", &self.value_field)
            .finish()
    }
}

impl Adapter {
    async fn get_connection(&self) -> crate::Result<&Surreal<Any>> {
        self.db
            .get_or_try_init(|| async {
                let namespace = self.namespace.as_str();
                let database = self.database.as_str();

                let db: Surreal<Any> = Surreal::init();
                db.connect(self.connection_string.clone())
                    .await
                    .map_err(parse_surrealdb_error)?;

                if !self.username.is_empty() && !self.password.is_empty() {
                    db.signin(Database {
                        namespace,
                        database,
                        username: self.username.as_str(),
                        password: self.password.as_str(),
                    })
                    .await
                    .map_err(parse_surrealdb_error)?;
                }
                db.use_ns(namespace)
                    .use_db(database)
                    .await
                    .map_err(parse_surrealdb_error)?;

                Ok(Arc::new(db))
            })
            .await
            .map(|v| v.as_ref())
    }
}

impl kv::Adapter for Adapter {
    fn metadata(&self) -> kv::Metadata {
        kv::Metadata::new(
            Scheme::Surrealdb,
            &self.table,
            Capability {
                read: true,
                write: true,
                ..Default::default()
            },
        )
    }

    async fn get(&self, path: &str) -> crate::Result<Option<Buffer>> {
        let query: String = if self.key_field == "id" {
            "SELECT type::field($value_field) FROM type::thing($table, $path)".to_string()
        } else {
            format!("SELECT type::field($value_field) FROM type::table($table) WHERE {} = $path LIMIT 1", self.key_field)
        };

        let mut result = self
            .get_connection()
            .await?
            .query(query)
            .bind(("namespace", "opendal"))
            .bind(("path", path))
            .bind(("table", self.table.as_str()))
            .bind(("value_field", self.value_field.as_str()))
            .await
            .map_err(parse_surrealdb_error)?;

        let value: Option<Vec<u8>> = result
            .take((0, self.value_field.as_str()))
            .map_err(parse_surrealdb_error)?;

        Ok(value.map(Buffer::from))
    }

    async fn set(&self, path: &str, value: Buffer) -> crate::Result<()> {
        let query = format!(
            "INSERT INTO {} ({}, {}) \
            VALUES ($path, $value) \
            ON DUPLICATE KEY UPDATE {} = $value",
            self.table, self.key_field, self.value_field, self.value_field
        );
        self.get_connection()
            .await?
            .query(query)
            .bind(("path", path))
            .bind(("value", value.to_vec()))
            .await
            .map_err(parse_surrealdb_error)?;
        Ok(())
    }

    async fn delete(&self, path: &str) -> crate::Result<()> {
        let query: String = if self.key_field == "id" {
            "DELETE FROM type::thing($table, $path)".to_string()
        } else {
            format!(
                "DELETE FROM type::table($table) WHERE {} = $path",
                self.key_field
            )
        };

        self.get_connection()
            .await?
            .query(query.as_str())
            .bind(("path", path))
            .bind(("table", self.table.as_str()))
            .await
            .map_err(parse_surrealdb_error)?;
        Ok(())
    }
}

fn parse_surrealdb_error(err: surrealdb::Error) -> Error {
    Error::new(ErrorKind::Unexpected, "unhandled error from surrealdb").set_source(err)
}
