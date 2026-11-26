use std::io::Result;
use std::time::Duration;
use futures::try_join;

use kaminari::{AsyncAccept, AsyncConnect, IOStream};
use kaminari::mix::{MixAccept, MixConnect};

use realm_io::{CopyBuffer, bidi_copy_buf_with_timeout, buf_size};

use crate::time::timeoutfut;

pub async fn run_relay<S: IOStream>(
    src: S,
    dst: S,
    ac: &MixAccept,
    cc: &MixConnect,
    connect_timeout: usize,
    half_close_timeout: Option<Duration>,
) -> Result<()> {
    macro_rules! hs_relay {
        ($ac: expr, $cc: expr) => {
            handshake_and_relay(src, dst, $ac, $cc, connect_timeout, half_close_timeout).await
        };
    }

    #[cfg(feature = "transport-boost")]
    {
        use MixConnect::*;
        if let Some(ac) = ac.as_plain() {
            return match cc {
                Plain(cc) => hs_relay!(ac, cc),
                Ws(cc) => hs_relay!(ac, cc),
                Tls(cc) => hs_relay!(ac, cc),
                Wss(cc) => hs_relay!(ac, cc),
            };
        }
    }

    #[cfg(feature = "transport-boost")]
    {
        use MixAccept::*;
        if let Some(cc) = cc.as_plain() {
            return match ac {
                Plain(ac) => hs_relay!(ac, cc),
                Ws(ac) => hs_relay!(ac, cc),
                Tls(ac) => hs_relay!(ac, cc),
                Wss(ac) => hs_relay!(ac, cc),
            };
        }
    }

    hs_relay!(ac, cc)
}

async fn handshake_and_relay<S, AC, CC>(
    src: S,
    dst: S,
    ac: &AC,
    cc: &CC,
    connect_timeout: usize,
    half_close_timeout: Option<Duration>,
) -> Result<()>
where
    S: IOStream,
    AC: AsyncAccept<S>,
    CC: AsyncConnect<S>,
{
    let mut buf1 = vec![0; buf_size()];
    let mut buf2 = vec![0; buf_size()];

    let handshake = async { try_join!(ac.accept(src, &mut buf1), cc.connect(dst, &mut buf2)) };
    let (mut src, mut dst) = timeoutfut(handshake, connect_timeout).await??;

    let buf1 = CopyBuffer::new(buf1);
    let buf2 = CopyBuffer::new(buf2);

    bidi_copy_buf_with_timeout(&mut src, &mut dst, buf1, buf2, half_close_timeout)
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaminari::mix::{MixClientConf, MixServerConf};
    use kaminari::opt::get_ws_conf;
    use tokio::io::duplex;
    use tokio::time::{Instant, timeout};

    #[tokio::test(start_paused = true)]
    async fn stalled_websocket_handshake_uses_tcp_timeout() {
        let ac = MixAccept::new(MixServerConf {
            ws: get_ws_conf("ws;host=localhost;path=/"),
            tls: None,
        });
        let cc = MixConnect::new(MixClientConf { ws: None, tls: None });
        assert!(matches!(ac, MixAccept::Ws(_)));

        let (_client, src) = duplex(64);
        let (_server, dst) = duplex(64);
        let start = Instant::now();
        let err = timeout(Duration::from_secs(2), run_relay(src, dst, &ac, &cc, 1, None))
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        assert!(start.elapsed() >= Duration::from_secs(1));

        let (_client, src) = duplex(64);
        let (_server, dst) = duplex(64);
        assert!(timeout(Duration::from_secs(2), run_relay(src, dst, &ac, &cc, 0, None))
            .await
            .is_err());
    }
}
