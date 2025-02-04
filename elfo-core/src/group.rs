use std::{fmt::Debug, future::Future, marker::PhantomData, sync::Arc};

use futures::future::BoxFuture;

use crate::{
    config::Config,
    context::Context,
    envelope::Envelope,
    exec::{Exec, ExecResult},
    object::{GroupHandle, GroupVisitor, Object},
    routers::Router,
    runtime::RuntimeManager,
    supervisor::Supervisor,
};

#[derive(Debug)]
pub struct ActorGroup<R, C> {
    restart_policy: RestartPolicy,
    termination_policy: TerminationPolicy,
    router: R,
    _config: PhantomData<C>,
}

impl ActorGroup<(), ()> {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            restart_policy: RestartPolicy::default(),
            termination_policy: TerminationPolicy::default(),
            router: (),
            _config: PhantomData,
        }
    }
}

impl<R, C> ActorGroup<R, C> {
    pub fn config<C1: Config>(self) -> ActorGroup<R, C1> {
        ActorGroup {
            restart_policy: self.restart_policy,
            termination_policy: self.termination_policy,
            router: self.router,
            _config: PhantomData,
        }
    }

    /// The behaviour on actor termination.
    /// `RestartPolicy::on_failures` is used by default.
    pub fn restart_policy(mut self, policy: RestartPolicy) -> Self {
        self.restart_policy = policy;
        self
    }

    /// The behaviour on the `Terminate` message.
    /// `TerminationPolicy::closing` is used by default.
    pub fn termination_policy(mut self, policy: TerminationPolicy) -> Self {
        self.termination_policy = policy;
        self
    }

    pub fn router<R1: Router<C>>(self, router: R1) -> ActorGroup<R1, C> {
        ActorGroup {
            restart_policy: self.restart_policy,
            termination_policy: self.termination_policy,
            router,
            _config: self._config,
        }
    }

    pub fn exec<X, O, ER>(self, exec: X) -> Blueprint
    where
        R: Router<C>,
        X: Fn(Context<C, R::Key>) -> O + Send + Sync + 'static,
        O: Future<Output = ER> + Send + 'static,
        ER: ExecResult,
        C: Config,
    {
        let run = move |ctx: Context, name: String, rt_manager: RuntimeManager| {
            let addr = ctx.group();
            let sv = Arc::new(Supervisor::new(
                ctx,
                name,
                exec,
                self.router,
                self.restart_policy,
                self.termination_policy,
                rt_manager,
            ));

            Object::new(addr, Box::new(Handle(sv)) as Box<dyn GroupHandle>)
        };

        Blueprint { run: Box::new(run) }
    }
}

struct Handle<R: Router<C>, C, X>(Arc<Supervisor<R, C, X>>);

impl<R, C, X> GroupHandle for Handle<R, C, X>
where
    R: Router<C>,
    X: Exec<Context<C, R::Key>>,
    <X::Output as Future>::Output: ExecResult,
    C: Config,
{
    fn handle(&self, envelope: Envelope, visitor: &mut dyn GroupVisitor) {
        self.0.handle(envelope, visitor)
    }

    fn finished(&self) -> BoxFuture<'static, ()> {
        self.0.finished()
    }
}

pub struct Blueprint {
    pub(crate) run: Box<dyn FnOnce(Context, String, RuntimeManager) -> Object>,
}

/// The behaviour on the `Terminate` message.
#[derive(Debug, Clone)]
pub struct TerminationPolicy {
    pub(crate) stop_spawning: bool,
    pub(crate) close_mailbox: bool,
}

impl Default for TerminationPolicy {
    fn default() -> Self {
        Self::closing()
    }
}

impl TerminationPolicy {
    /// On `Terminate`:
    /// * A supervisor stops spawning new actors.
    /// * New messages are not accepted more.
    /// * Mailboxes are closed.
    ///
    /// This behaviour is used by default.
    pub fn closing() -> Self {
        Self {
            stop_spawning: true,
            close_mailbox: true,
        }
    }

    /// On `Terminate`:
    /// * A supervisor stops spawning new actors.
    /// * The `Terminate` message can be handled by actors manually.
    /// * Mailboxes receive messages (use `Context::close()` to stop it).
    pub fn manually() -> Self {
        Self {
            stop_spawning: true,
            close_mailbox: false,
        }
    }

    // TODO: add `stop_spawning`?
}

/// The behaviour on actor termination.
#[derive(Debug, Clone)]
pub struct RestartPolicy {
    pub(crate) mode: RestartMode,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self::on_failures()
    }
}

#[derive(Debug, Clone)]
pub(crate) enum RestartMode {
    Always,
    OnFailures,
    Never,
}

impl RestartPolicy {
    pub fn always() -> Self {
        Self {
            mode: RestartMode::Always,
        }
    }

    pub fn on_failures() -> Self {
        Self {
            mode: RestartMode::OnFailures,
        }
    }

    pub fn never() -> Self {
        Self {
            mode: RestartMode::Never,
        }
    }
}
