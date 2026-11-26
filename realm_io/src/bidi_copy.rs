use std::io::Result;
#[cfg(target_os = "linux")]
use std::io::ErrorKind;
use std::pin::Pin;
use std::task::{Context, Poll, ready};
use std::future::Future;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::{self, Instant, Sleep};

use super::{AsyncIOBuf, CopyBuffer};

enum TransferState<B, SR, SW> {
    Running(CopyBuffer<B, SR, SW>),
    ShuttingDown(u64),
    Done(u64),
}

impl<B, SR, SW> TransferState<B, SR, SW> {
    #[cfg(target_os = "linux")]
    fn can_retry(&self) -> bool {
        // A positive read leaves cap nonzero even after the pipe drains.
        matches!(self, Self::Running(buf) if buf.cap == 0 && !buf.read_done)
    }

    fn written(&self) -> u64 {
        match self {
            Self::Running(buf) => buf.amt,
            Self::ShuttingDown(count) | Self::Done(count) => *count,
        }
    }
}

struct HalfClose {
    timeout: Option<Duration>,
    timer: Option<(u64, Pin<Box<Sleep>>)>,
}

impl HalfClose {
    fn poll(&mut self, cx: &mut Context<'_>, written: u64) -> Poll<u64> {
        let Some(timeout) = self.timeout else {
            return Poll::Pending;
        };
        if timeout.is_zero() {
            return Poll::Ready(0);
        }

        let (last_written, timer) = self
            .timer
            .get_or_insert_with(|| (written, Box::pin(time::sleep(timeout))));
        if written != *last_written {
            *last_written = written;
            if let Some(deadline) = Instant::now().checked_add(timeout) {
                timer.as_mut().reset(deadline);
            }
        }
        // Register the waker even when the sleep was just created or reset.
        timer.as_mut().poll(cx).map(|()| written)
    }
}

fn transfer<B, SL, SR>(
    cx: &mut Context<'_>,
    state: &mut TransferState<B, SL, SR>,
    r: &mut <CopyBuffer<B, SL, SR> as AsyncIOBuf>::StreamR,
    w: &mut <CopyBuffer<B, SL, SR> as AsyncIOBuf>::StreamW,
) -> Poll<Result<u64>>
where
    B: Unpin,
    SL: AsyncRead + AsyncWrite + Unpin,
    SR: AsyncRead + AsyncWrite + Unpin,
    CopyBuffer<B, SL, SR>: AsyncIOBuf,
    CopyBuffer<B, SR, SL>: AsyncIOBuf,
{
    loop {
        match state {
            TransferState::Running(buf) => {
                let count = ready!(buf.poll_copy(cx, r, w))?;

                *state = TransferState::ShuttingDown(count);
            }
            TransferState::ShuttingDown(count) => {
                ready!(Pin::new(&mut *w).poll_shutdown(cx))?;

                *state = TransferState::Done(*count);
            }
            TransferState::Done(count) => return Poll::Ready(Ok(*count)),
        }
    }
}

fn transfer2<B, SL, SR>(
    cx: &mut Context<'_>,
    state: &mut TransferState<B, SR, SL>, // reverse
    r: &mut <CopyBuffer<B, SL, SR> as AsyncIOBuf>::StreamW,
    w: &mut <CopyBuffer<B, SL, SR> as AsyncIOBuf>::StreamR,
) -> Poll<Result<u64>>
where
    B: Unpin,
    SL: AsyncRead + AsyncWrite + Unpin,
    SR: AsyncRead + AsyncWrite + Unpin,
    CopyBuffer<B, SL, SR>: AsyncIOBuf,
    CopyBuffer<B, SR, SL>: AsyncIOBuf,
{
    // type equality constraints will save this (one day)!
    let r: &mut <CopyBuffer<B, SR, SL> as AsyncIOBuf>::StreamR = unsafe { std::mem::transmute(r) };
    let w: &mut <CopyBuffer<B, SR, SL> as AsyncIOBuf>::StreamW = unsafe { std::mem::transmute(w) };
    loop {
        match state {
            TransferState::Running(buf) => {
                let count = ready!(buf.poll_copy(cx, r, w))?;

                *state = TransferState::ShuttingDown(count);
            }
            TransferState::ShuttingDown(count) => {
                ready!(Pin::new(&mut *w).poll_shutdown(cx))?;

                *state = TransferState::Done(*count);
            }
            TransferState::Done(count) => return Poll::Ready(Ok(*count)),
        }
    }
}

struct BidiCopy<'a, B, SL, SR>
where
    B: Unpin,
    SL: AsyncRead + AsyncWrite + Unpin,
    SR: AsyncRead + AsyncWrite + Unpin,
    CopyBuffer<B, SL, SR>: AsyncIOBuf,
    CopyBuffer<B, SR, SL>: AsyncIOBuf,
{
    a: &'a mut <CopyBuffer<B, SL, SR> as AsyncIOBuf>::StreamR,
    b: &'a mut <CopyBuffer<B, SL, SR> as AsyncIOBuf>::StreamW,
    a_to_b: TransferState<B, SL, SR>,
    b_to_a: TransferState<B, SR, SL>,
    half_close: HalfClose,
}

impl<'a, B, SL, SR> BidiCopy<'a, B, SL, SR>
where
    B: Unpin,
    SL: AsyncRead + AsyncWrite + Unpin,
    SR: AsyncRead + AsyncWrite + Unpin,
    CopyBuffer<B, SL, SR>: AsyncIOBuf,
    CopyBuffer<B, SR, SL>: AsyncIOBuf,
{
    fn new(
        a: &'a mut <CopyBuffer<B, SL, SR> as AsyncIOBuf>::StreamR,
        b: &'a mut <CopyBuffer<B, SL, SR> as AsyncIOBuf>::StreamW,
        a_to_b: CopyBuffer<B, SL, SR>,
        b_to_a: CopyBuffer<B, SR, SL>,
        timeout: Option<Duration>,
    ) -> Self {
        Self {
            a,
            b,
            a_to_b: TransferState::Running(a_to_b),
            b_to_a: TransferState::Running(b_to_a),
            half_close: HalfClose { timeout, timer: None },
        }
    }
}

impl<'a, B, SL, SR> Future for BidiCopy<'a, B, SL, SR>
where
    B: Unpin,
    SL: AsyncRead + AsyncWrite + Unpin,
    SR: AsyncRead + AsyncWrite + Unpin,
    CopyBuffer<B, SL, SR>: AsyncIOBuf,
    CopyBuffer<B, SR, SL>: AsyncIOBuf,
{
    type Output = Result<(u64, u64)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let BidiCopy {
            a,
            b,
            a_to_b,
            b_to_a,
            half_close,
        } = self.get_mut();

        let forward = transfer(cx, a_to_b, a, b)?;
        let reverse = transfer2::<B, SL, SR>(cx, b_to_a, b, a)?;

        match (forward, reverse) {
            (Poll::Ready(forward), Poll::Ready(reverse)) => Poll::Ready(Ok((forward, reverse))),
            (Poll::Ready(forward), Poll::Pending) => half_close
                .poll(cx, b_to_a.written())
                .map(|reverse| Ok((forward, reverse))),
            (Poll::Pending, Poll::Ready(reverse)) => half_close
                .poll(cx, a_to_b.written())
                .map(|forward| Ok((forward, reverse))),
            (Poll::Pending, Poll::Pending) => Poll::Pending,
        }
    }
}

/// Copy data bidirectionally between two streams with provided buffer.
pub async fn bidi_copy_buf<B, SR, SW>(
    a: &mut <CopyBuffer<B, SR, SW> as AsyncIOBuf>::StreamR,
    b: &mut <CopyBuffer<B, SR, SW> as AsyncIOBuf>::StreamW,
    a_to_b_buf: CopyBuffer<B, SR, SW>,
    b_to_a_buf: CopyBuffer<B, SW, SR>,
) -> Result<(u64, u64)>
where
    B: Unpin,
    SR: AsyncRead + AsyncWrite + Unpin,
    SW: AsyncRead + AsyncWrite + Unpin,
    CopyBuffer<B, SR, SW>: AsyncIOBuf,
    CopyBuffer<B, SW, SR>: AsyncIOBuf,
{
    let timeout = cfg!(feature = "brutal-shutdown").then_some(Duration::ZERO);
    bidi_copy_buf_with_timeout(a, b, a_to_b_buf, b_to_a_buf, timeout).await
}

/// Copy bidirectionally with a configurable idle period after one writer shuts down.
/// `None` waits for both directions; `Some(Duration::ZERO)` returns immediately
/// when either direction completes.
pub async fn bidi_copy_buf_with_timeout<B, SR, SW>(
    a: &mut <CopyBuffer<B, SR, SW> as AsyncIOBuf>::StreamR,
    b: &mut <CopyBuffer<B, SR, SW> as AsyncIOBuf>::StreamW,
    a_to_b_buf: CopyBuffer<B, SR, SW>,
    b_to_a_buf: CopyBuffer<B, SW, SR>,
    timeout: Option<Duration>,
) -> Result<(u64, u64)>
where
    B: Unpin,
    SR: AsyncRead + AsyncWrite + Unpin,
    SW: AsyncRead + AsyncWrite + Unpin,
    CopyBuffer<B, SR, SW>: AsyncIOBuf,
    CopyBuffer<B, SW, SR>: AsyncIOBuf,
{
    BidiCopy::new(a, b, a_to_b_buf, b_to_a_buf, timeout).await
}

/// Copy bidirectionally with existing pipes and the given half-close idle timeout.
/// `Ok(None)` means splice returned `InvalidInput` before either direction read
/// data or reached EOF, so the caller can still relay the streams in memory.
#[cfg(target_os = "linux")]
pub async fn try_bidi_zero_copy_with_pipes<A, B>(
    a: &mut A,
    b: &mut B,
    forward: crate::Pipe,
    reverse: crate::Pipe,
    timeout: Option<Duration>,
) -> Result<Option<(u64, u64)>>
where
    A: AsyncRead + AsyncWrite + crate::AsyncRawIO + Unpin,
    B: AsyncRead + AsyncWrite + crate::AsyncRawIO + Unpin,
{
    let mut copy = BidiCopy::new(a, b, CopyBuffer::new(forward), CopyBuffer::new(reverse), timeout);
    match (&mut copy).await {
        Ok(counts) => Ok(Some(counts)),
        Err(e) if e.kind() == ErrorKind::InvalidInput && copy.a_to_b.can_retry() && copy.b_to_a.can_retry() => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::os::fd::{AsRawFd, RawFd};
    use std::pin::pin;
    use std::task::Waker;
    use tokio::io::{Interest, ReadBuf};
    use crate::{AsyncRawIO, Pipe};

    type ReadStep = Poll<std::result::Result<usize, ErrorKind>>;
    const EINVAL: ReadStep = Poll::Ready(Err(ErrorKind::InvalidInput));

    #[derive(Default)]
    struct SpliceStream {
        reads: Cell<&'static [ReadStep]>,
        write_fails: bool,
        flush_stalls: bool,
    }

    impl SpliceStream {
        fn with_reads(reads: &'static [ReadStep]) -> Self {
            Self {
                reads: Cell::new(reads),
                ..Self::default()
            }
        }
    }

    impl AsRawFd for SpliceStream {
        fn as_raw_fd(&self) -> RawFd {
            unreachable!("the stream supplies scripted splice results")
        }
    }

    impl AsyncRead for SpliceStream {
        fn poll_read(self: Pin<&mut Self>, _: &mut Context<'_>, _: &mut ReadBuf<'_>) -> Poll<Result<()>> {
            unreachable!("splice reads through AsyncRawIO")
        }
    }

    impl AsyncWrite for SpliceStream {
        fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, _: &[u8]) -> Poll<Result<usize>> {
            unreachable!("splice writes through AsyncRawIO")
        }

        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<()>> {
            if self.flush_stalls {
                Poll::Pending
            } else {
                Poll::Ready(Ok(()))
            }
        }

        fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    impl AsyncRawIO for SpliceStream {
        fn x_poll_read_ready(&self, _: &mut Context<'_>) -> Poll<Result<()>> {
            unreachable!()
        }

        fn x_poll_write_ready(&self, _: &mut Context<'_>) -> Poll<Result<()>> {
            unreachable!()
        }

        fn x_try_io<R>(&self, _: Interest, _: impl FnOnce() -> Result<R>) -> Result<R> {
            unreachable!()
        }

        fn poll_read_raw<S>(&self, _: &mut Context<'_>, _: S) -> Poll<Result<usize>>
        where
            S: FnMut() -> isize,
        {
            let Some((&next, rest)) = self.reads.get().split_first() else {
                return Poll::Pending;
            };
            self.reads.set(rest);
            next.map(|result| result.map_err(Into::into))
        }

        fn poll_write_raw<S>(&self, _: &mut Context<'_>, _: S) -> Poll<Result<usize>>
        where
            S: FnMut() -> isize,
        {
            if self.write_fails {
                Poll::Ready(Err(ErrorKind::InvalidInput.into()))
            } else {
                Poll::Ready(Ok(1))
            }
        }
    }

    async fn try_splice(a: &mut SpliceStream, b: &mut SpliceStream) -> Result<Option<(u64, u64)>> {
        try_bidi_zero_copy_with_pipes(a, b, Pipe::new()?, Pipe::new()?, None).await
    }

    #[test]
    fn can_retry_if_the_first_splice_returns_invalid_input() {
        for reverse in [false, true] {
            let mut a = SpliceStream::with_reads(if reverse { &[] } else { &[EINVAL] });
            let mut b = SpliceStream::with_reads(if reverse { &[EINVAL] } else { &[] });
            let result = pin!(try_splice(&mut a, &mut b)).poll(&mut Context::from_waker(Waker::noop()));
            assert!(matches!(result, Poll::Ready(Ok(None))));
        }
    }

    #[test]
    fn cannot_retry_after_reading_into_a_pipe_or_reaching_eof() {
        const DATA: &[ReadStep] = &[Poll::Ready(Ok(1))];
        const EOF: &[ReadStep] = &[Poll::Ready(Ok(0))];
        for (forward_read, reverse_read, write_fails, flush_stalls) in [
            (DATA, &[][..], true, false),
            (EOF, &[EINVAL][..], false, false),
            (EOF, &[EINVAL][..], false, true),
        ] {
            let mut a = SpliceStream::with_reads(forward_read);
            let mut b = SpliceStream {
                reads: Cell::new(reverse_read),
                write_fails,
                flush_stalls,
            };
            let result = pin!(try_splice(&mut a, &mut b)).poll(&mut Context::from_waker(Waker::noop()));
            assert!(matches!(result, Poll::Ready(Err(e)) if e.kind() == ErrorKind::InvalidInput));
        }
    }

    #[test]
    fn cannot_retry_after_transferring_data_in_the_other_direction() {
        let mut a = SpliceStream::with_reads(&[Poll::Pending, EINVAL]);
        let mut b = SpliceStream::with_reads(&[Poll::Ready(Ok(1))]);
        let mut copy = pin!(try_splice(&mut a, &mut b));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(copy.as_mut().poll(&mut cx).is_pending());
        let result = copy.poll(&mut cx);
        assert!(matches!(result, Poll::Ready(Err(e)) if e.kind() == ErrorKind::InvalidInput));
    }
}
