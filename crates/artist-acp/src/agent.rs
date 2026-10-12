use crate::{
    Options,
    session::{Session, internal, invalid},
};
use agent_client_protocol::{
    Agent, AgentSideConnection, AuthenticateRequest, AuthenticateResponse, CancelNotification,
    ContentBlock, Error, Implementation, InitializeRequest, InitializeResponse, NewSessionRequest,
    NewSessionResponse, PromptRequest, PromptResponse, ProtocolVersion, Result,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

pub struct ArtistAgent {
    options: Options,
    initialized: Cell<bool>,
    next_id: Cell<u64>,
    sessions: RefCell<HashMap<String, Rc<Session>>>,
    connection: RefCell<Option<Rc<AgentSideConnection>>>,
}

impl ArtistAgent {
    pub fn new(options: Options) -> Self {
        Self {
            options,
            initialized: Cell::new(false),
            next_id: Cell::new(0),
            sessions: RefCell::new(HashMap::new()),
            connection: RefCell::new(None),
        }
    }

    pub fn set_connection(&self, connection: Rc<AgentSideConnection>) {
        *self.connection.borrow_mut() = Some(connection);
    }

    pub fn disconnect(&self) {
        for session in self.sessions.borrow().values() {
            session.cancel();
        }
        self.connection.borrow_mut().take();
    }

    fn session(&self, id: &str) -> Result<Rc<Session>> {
        self.sessions
            .borrow()
            .get(id)
            .cloned()
            .ok_or_else(|| invalid("unknown session"))
    }
}

#[async_trait::async_trait(?Send)]
impl Agent for ArtistAgent {
    async fn initialize(&self, args: InitializeRequest) -> Result<InitializeResponse> {
        if self.initialized.get() {
            return Err(invalid("connection already initialized"));
        }
        self.initialized
            .set(args.protocol_version == ProtocolVersion::V1);
        // Default capabilities: text/resource links, no persistence, MCP, or rich media.
        Ok(InitializeResponse::new(ProtocolVersion::V1)
            .agent_info(Implementation::new("artist", env!("CARGO_PKG_VERSION"))))
    }

    async fn authenticate(&self, _: AuthenticateRequest) -> Result<AuthenticateResponse> {
        // Provider authentication belongs to the Lua configuration/environment.
        Err(Error::method_not_found())
    }

    async fn new_session(&self, args: NewSessionRequest) -> Result<NewSessionResponse> {
        if !self.initialized.get() {
            return Err(invalid("initialize with ACP v1 before creating sessions"));
        }
        if !args.mcp_servers.is_empty() {
            return Err(invalid("MCP servers are not supported"));
        }
        let session = Session::new(&self.options, args.cwd).await?;
        let next = self
            .next_id
            .get()
            .checked_add(1)
            .ok_or_else(|| internal("session IDs exhausted"))?;
        self.next_id.set(next);
        let id = format!("artist-{next}");
        self.sessions
            .borrow_mut()
            .insert(id.clone(), Rc::new(session));
        Ok(NewSessionResponse::new(id))
    }

    async fn prompt(&self, args: PromptRequest) -> Result<PromptResponse> {
        let session = self.session(args.session_id.0.as_ref())?;
        let mut parts = Vec::new();
        for block in args.prompt {
            match block {
                ContentBlock::Text(text) => parts.push(text.text),
                ContentBlock::ResourceLink(link) => {
                    parts.push(format!("Resource: {} ({})", link.name, link.uri));
                }
                _ => return Err(invalid("only text and resource links are supported")),
            }
        }
        let connection = self
            .connection
            .borrow()
            .clone()
            .ok_or_else(|| internal("client disconnected"))?;
        session
            .prompt(args.session_id, parts.join("\n"), connection.as_ref())
            .await
    }

    async fn cancel(&self, args: CancelNotification) -> Result<()> {
        self.session(args.session_id.0.as_ref())?.cancel();
        Ok(())
    }
}
