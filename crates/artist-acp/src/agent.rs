use crate::{
    Options,
    session::{Session, internal, invalid},
};
use agent_client_protocol::{
    Agent, AgentSideConnection, AuthenticateRequest, AuthenticateResponse, CancelNotification,
    ContentBlock, Error, Implementation, InitializeRequest, InitializeResponse, NewSessionRequest,
    NewSessionResponse, PromptRequest, PromptResponse, ProtocolVersion, Result,
    AgentCapabilities, Client, LoadSessionRequest, LoadSessionResponse, SessionNotification,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

pub struct ArtistAgent {
    options: Options,
    initialized: Cell<bool>,
    sessions: RefCell<HashMap<String, Rc<Session>>>,
    connection: RefCell<Option<Rc<AgentSideConnection>>>,
}

impl ArtistAgent {
    pub fn new(options: Options) -> Self {
        Self {
            options,
            initialized: Cell::new(false),
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
        Ok(InitializeResponse::new(ProtocolVersion::V1)
            .agent_capabilities(AgentCapabilities::new().load_session(true))
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
        let session = Session::new(&self.options, args.cwd, None).await?;
        let id = session.store.id.clone();
        self.sessions
            .borrow_mut()
            .insert(id.clone(), Rc::new(session));
        Ok(NewSessionResponse::new(id))
    }

    async fn load_session(&self, args: LoadSessionRequest) -> Result<LoadSessionResponse> {
        if !self.initialized.get() {
            return Err(invalid("initialize with ACP v1 before loading sessions"));
        }
        if !args.mcp_servers.is_empty() {
            return Err(invalid("MCP servers are not supported"));
        }
        let id = args.session_id.0.to_string();
        if self.sessions.borrow().contains_key(&id) {
            return Err(invalid("session is already loaded"));
        }
        let session = Session::new(&self.options, args.cwd, Some(&id)).await?;
        let connection = self.connection.borrow().clone()
            .ok_or_else(|| internal("client disconnected"))?;
        for entry in session.store.transcript().await.map_err(internal)? {
            for update in crate::replay::updates(&entry) {
                connection.session_notification(SessionNotification::new(args.session_id.clone(), update)).await?;
            }
        }
        self.sessions.borrow_mut().insert(id, Rc::new(session));
        Ok(LoadSessionResponse::new())
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
