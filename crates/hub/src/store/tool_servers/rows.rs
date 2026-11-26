use protocol::model::{Rev, ToolServer, ToolServerAuth, ToolServerId, ToolServerName, ToolServerUrl};
use rusqlite::{Connection, OptionalExtension as _, Row, params};

use super::super::{ByColumn, Columns, Store, StoreError, columns, json_at, lock, rev_of};
use crate::login_files::LoginFile;

columns! {
    /// The columns every read of a tool server's row takes, as [`server_at`] and [`secret_at`] read them.
    struct ServerColumns { id, name, url, auth, secret_file, rev }
}

/// A tool server a person is adding.
#[derive(Clone, Copy)]
pub(crate) struct AddingToolServer<'a> {
    pub(crate) id: &'a ToolServerId,
    pub(crate) name: &'a ToolServerName,
    pub(crate) url: &'a ToolServerUrl,
    pub(crate) auth: &'a ToolServerAuth,
    /// Where what the server knows the person by is, or `None` for one that doesn't know them.
    pub(crate) secret: Option<&'a LoginFile>,
}

/// What a person changes of a tool server, with what isn't here left as it is.
#[derive(Clone, Copy, Default)]
pub(crate) struct EditingToolServer<'a> {
    pub(crate) name: Option<&'a ToolServerName>,
    pub(crate) url: Option<&'a ToolServerUrl>,
    pub(crate) known_by: Option<KnownBy<'a>>,
}

/// How a tool server is to know the person, and where what it knows them by is, as [`AddingToolServer`] says.
#[derive(Clone, Copy)]
pub(crate) struct KnownBy<'a> {
    pub(crate) auth: &'a ToolServerAuth,
    pub(crate) secret: Option<&'a LoginFile>,
}

/// A row's record. A way of knowing the person this build can't read, which only a newer build's row could hold, is
/// `Unknown`.
fn server_at(row: &Row<'_>) -> rusqlite::Result<ToolServer> {
    let c = ServerColumns::at(0);
    Ok(ToolServer {
        id: row.at::<String>(c.id)?.into(),
        name: ToolServerName::stored(row.at(c.name)?),
        url: ToolServerUrl::stored(row.at(c.url)?),
        auth: json_at(row, c.auth)?.unwrap_or(ToolServerAuth::Unknown),
        rev: rev_of(row, c.rev)?,
    })
}

/// Which file a row says its secret is in, `None` for one the hub doesn't make, and for a row that says none.
fn secret_at(row: &Row<'_>) -> rusqlite::Result<Option<LoginFile>> {
    let c = ServerColumns::at(0);
    Ok(row.at::<Option<String>>(c.secret_file)?.as_deref().and_then(LoginFile::parse))
}

/// Every tool server, in the order `protocol` lists them in.
pub(in super::super) fn tool_servers(conn: &Connection) -> rusqlite::Result<Vec<ToolServer>> {
    let mut statement = conn.prepare_cached(&format!("SELECT {} FROM tool_servers", ServerColumns::LIST))?;
    let mut servers = statement.query_map([], server_at)?.collect::<rusqlite::Result<Vec<_>>>()?;
    servers.sort_by(ToolServer::by_name);
    Ok(servers)
}

fn row_of(conn: &Connection, id: &ToolServerId) -> rusqlite::Result<Option<(ToolServer, Option<LoginFile>)>> {
    let select = format!("SELECT {} FROM tool_servers WHERE id = ?", ServerColumns::LIST);
    conn.query_row(&select, [id.as_str()], |row| Ok((server_at(row)?, secret_at(row)?))).optional()
}

impl Store {
    pub(crate) fn tool_servers(&self) -> Result<Vec<ToolServer>, StoreError> {
        Ok(tool_servers(&lock(&self.reader))?)
    }

    /// The tool server `id`, if the hub has it, and the file of what it knows the person by, `None` for one that
    /// doesn't know them, and for a file the hub doesn't make.
    pub(crate) fn tool_server(&self, id: &ToolServerId) -> Result<Option<(ToolServer, Option<LoginFile>)>, StoreError> {
        Ok(row_of(&lock(&self.reader), id)?)
    }

    /// Each tool server that names a file, with the file, or `None` for one the hub doesn't make. A server that doesn't
    /// know the person names none, so it isn't here, and tidying leaves it.
    pub(crate) fn tool_servers_with_secrets(&self) -> Result<Vec<(ToolServer, Option<LoginFile>)>, StoreError> {
        let conn = lock(&self.reader);
        let select = format!("SELECT {} FROM tool_servers WHERE secret_file IS NOT NULL", ServerColumns::LIST);
        let mut statement = conn.prepare_cached(&select)?;
        let rows = statement.query_map([], |row| Ok((server_at(row)?, secret_at(row)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Keeps the record of the tool server `adding`, whose secret is in its file already, and answers with it. Its
    /// commit is on disk before this returns, so a secret the hub has said it holds isn't lost to a power cut.
    pub(crate) fn add_tool_server(&self, adding: &AddingToolServer<'_>) -> Result<ToolServer, StoreError> {
        let AddingToolServer { id, name, url, auth, secret } = *adding;
        let auth = serde_json::to_string(auth)?;
        self.lasting(|tx| {
            tx.execute(
                "INSERT INTO tool_servers (id, name, url, auth, secret_file) VALUES (?, ?, ?, ?, ?)",
                params![id.as_str(), name.as_str(), url.as_str(), auth, secret.map(LoginFile::as_str)],
            )?;
            Ok(row_of(tx, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?.0)
        })
    }

    /// Returns the tool server as `editing` left it, or `None` when it's all so already, or there's no such server. All
    /// of it is written at once, with the file of what the server knows the person by, so a hub that stops leaves the
    /// server as it was or as it was edited. Its commit is on disk before this returns, since the file before is let go
    /// of then.
    pub(crate) fn edit_tool_server(
        &self,
        id: &ToolServerId,
        editing: &EditingToolServer<'_>,
    ) -> Result<Option<ToolServer>, StoreError> {
        let EditingToolServer { name, url, known_by } = *editing;
        let auth = known_by.map(|known_by| serde_json::to_string(known_by.auth)).transpose()?;
        let secret = known_by.and_then(|known_by| known_by.secret).map(LoginFile::as_str);
        self.lasting(|tx| {
            let update = "UPDATE tool_servers
                SET name = coalesce(?2, name), url = coalesce(?3, url), auth = coalesce(?4, auth),
                    secret_file = iif(?4 IS NULL, secret_file, ?5)
                WHERE id = ?1 AND (name IS NOT coalesce(?2, name) OR url IS NOT coalesce(?3, url)
                    OR auth IS NOT coalesce(?4, auth) OR secret_file IS NOT iif(?4 IS NULL, secret_file, ?5))";
            let (name, url) = (name.map(ToolServerName::as_str), url.map(ToolServerUrl::as_str));
            if tx.execute(update, params![id.as_str(), name, url, auth, secret])? == 0 {
                return Ok(None);
            }
            Ok(row_of(tx, id)?.map(|(server, _)| server))
        })
    }

    /// Returns the tool server as knowing the person by `auth` left it, or `None` when it does already, or there's no
    /// such server.
    pub(crate) fn set_tool_server_auth(
        &self,
        id: &ToolServerId,
        auth: &ToolServerAuth,
    ) -> Result<Option<ToolServer>, StoreError> {
        let auth = serde_json::to_string(auth)?;
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let update = "UPDATE tool_servers SET auth = ?1 WHERE id = ?2 AND auth IS NOT ?1";
        if tx.execute(update, params![auth, id.as_str()])? == 0 {
            return Ok(None);
        }
        let changed = row_of(&tx, id)?.map(|(server, _)| server);
        tx.commit()?;
        Ok(changed)
    }

    /// Removes the tool server `id`'s record, whose secret the hub has let go of, and answers with the revision of its
    /// removal, or `None` when there was no such server.
    pub(crate) fn remove_tool_server(&self, id: &ToolServerId) -> Result<Option<Rev>, StoreError> {
        self.delete("tool_servers", id.as_str())
    }
}
