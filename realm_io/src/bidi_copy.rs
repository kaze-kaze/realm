use std::io::Result;
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
    BidiCopy {
        a,
        b,
        a_to_b: TransferState::Running(a_to_b_buf),
        b_to_a: TransferState::Running(b_to_a_buf),
        half_close: HalfClose { timeout, timer: None },
    }
    .await
}
