use spinel::PosixSpinelHostHandle;

/// Subscribe to every frame broadcast and log the traffic.
pub async fn run_monitor(actor: PosixSpinelHostHandle) {
    let mut reset_broadcast_rx = actor.subscribe_reset_msg().await.unwrap();
    let mut debug_broadcast_rx = actor.subscribe_debug_broadcast().await.unwrap();
    let mut net_broadcast_rx = actor.subscribe_net_broadcast().await.unwrap();
    let mut net_insecure_broadcast_rx = actor.subscribe_net_insecure_broadcast().await.unwrap();
    let mut log_broadcast_rx = actor.subscribe_log_broadcast().await.unwrap();

    loop {
        tokio::select! {
            frame = reset_broadcast_rx.recv() => {
                log::trace!("reset broadcast received: {:?}", frame);
            }
            frame = debug_broadcast_rx.recv() => {
                log::trace!("debug broadcast received: {:?}", frame);
            }
            frame = net_broadcast_rx.recv() => {
                log::trace!("net broadcast received: {:?}", frame);
            }
            frame = net_insecure_broadcast_rx.recv() => {
                log::trace!("net insecure broadcast received: {:?}", frame);
            }
            frame = log_broadcast_rx.recv() => {
                log::trace!("log broadcast received: {:?}", frame);
            }
        }
    }
}
