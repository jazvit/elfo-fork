use crate::{
    addr::Addr,
    address_book::AddressBook,
    message::{AnyMessage, Message},
    request_table::ResponseToken,
    trace_id::TraceId,
};

// TODO: use granular messages instead of `SmallBox`.
#[derive(Debug)]
pub struct Envelope<M = AnyMessage> {
    trace_id: TraceId,
    kind: MessageKind,
    message: M,
}

assert_impl_all!(Envelope: Send);
assert_eq_size!(Envelope, [u8; 128]);

#[derive(Debug)]
pub(crate) enum MessageKind {
    Regular { sender: Addr },
    RequestAny(ResponseToken<()>),
    RequestAll(ResponseToken<()>),
}

impl<M: Message> Envelope<M> {
    pub(crate) fn new(message: M, kind: MessageKind) -> Self {
        Self {
            trace_id: TraceId::new(1).unwrap(), // TODO: load trace_id.
            kind,
            message,
        }
    }

    #[inline]
    pub fn sender(&self) -> Addr {
        match &self.kind {
            MessageKind::Regular { sender } => *sender,
            MessageKind::RequestAny(token) => token.sender,
            MessageKind::RequestAll(token) => token.sender,
        }
    }

    pub(crate) fn upcast(self) -> Envelope {
        Envelope {
            trace_id: self.trace_id,
            kind: self.kind,
            message: AnyMessage::new(self.message),
        }
    }

    // TODO: make `pub` for regular messages.
    pub(crate) fn into_message(self) -> M {
        self.message
    }
}

impl Envelope {
    #[inline]
    pub fn is<M: Message>(&self) -> bool {
        self.message.is::<M>()
    }

    pub(crate) fn do_downcast<M: Message>(self) -> Envelope<M> {
        let message = self.message.downcast::<M>().expect("cannot downcast");
        Envelope {
            trace_id: self.trace_id,
            kind: self.kind,
            message,
        }
    }

    // XXX: why does `Envelope` know about `AddressBook`?
    pub(crate) fn duplicate(&self, book: &AddressBook) -> Option<Self> {
        Some(Self {
            trace_id: self.trace_id,
            kind: match &self.kind {
                MessageKind::Regular { sender } => MessageKind::Regular { sender: *sender },
                MessageKind::RequestAny(token) => {
                    let object = book.get(token.sender)?;
                    let token = object.as_actor()?.request_table.clone_token(token)?;
                    MessageKind::RequestAny(token)
                }
                MessageKind::RequestAll(token) => {
                    let object = book.get(token.sender)?;
                    let token = object.as_actor()?.request_table.clone_token(token)?;
                    MessageKind::RequestAll(token)
                }
            },
            message: self.message.clone(),
        })
    }

    pub(crate) fn set_message<M: Message>(&mut self, message: M) {
        self.message = AnyMessage::new(message);
    }
}

// Extra traits to support both owned and borrowed usages of `msg!(..)`.

pub trait EnvelopeOwned {
    fn unpack_regular(self) -> AnyMessage;
    fn unpack_request<T>(self) -> (AnyMessage, ResponseToken<T>);
}

pub trait EnvelopeBorrowed {
    fn unpack_regular(&self) -> &AnyMessage;
}

impl EnvelopeOwned for Envelope {
    #[inline]
    fn unpack_regular(self) -> AnyMessage {
        self.message
    }

    #[inline]
    fn unpack_request<T>(self) -> (AnyMessage, ResponseToken<T>) {
        match self.kind {
            MessageKind::RequestAny(token) => (self.message, token.into_typed()),
            MessageKind::RequestAll(token) => (self.message, token.into_typed()),
            _ => unreachable!(),
        }
    }
}

impl EnvelopeBorrowed for Envelope {
    #[inline]
    fn unpack_regular(&self) -> &AnyMessage {
        &self.message
    }
}

pub trait AnyMessageOwned {
    fn downcast2<M: Message>(self) -> M;
}

pub trait AnyMessageBorrowed {
    fn downcast2<M: Message>(&self) -> &M;
}

impl AnyMessageOwned for AnyMessage {
    #[inline]
    fn downcast2<M: Message>(self) -> M {
        self.downcast::<M>().expect("cannot downcast")
    }
}

impl AnyMessageBorrowed for AnyMessage {
    #[inline]
    fn downcast2<M: Message>(&self) -> &M {
        self.downcast_ref::<M>().expect("cannot downcast")
    }
}
