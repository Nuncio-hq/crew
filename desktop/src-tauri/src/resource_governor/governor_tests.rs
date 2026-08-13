use super::*;
use crate::resource_governor::simctl::{ListedDevice, RecordingSimctl};

fn chan_a() -> &'static str {
    "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50"
}
fn chan_b() -> &'static str {
    "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
}

fn governor() -> ResourceGovernor {
    let mut g = ResourceGovernor::with_clock(Clock::new(1_000));
    g.set_bridge_for_test(BridgeAvailability::Missing {
        install_hint: "brew install baguette".into(),
    });
    g
}

#[test]
fn ensure_does_not_boot() {
    let mut g = governor();
    let simctl = RecordingSimctl::default();
    let holding = g
        .ensure_device(chan_a(), Some("glowmax"), None, None, &simctl)
        .expect("ensure");
    assert_eq!(holding.lifecycle, DeviceLifecycle::Absent);
    assert_eq!(holding.device_name, "crew-9a1657ac");
    assert!(simctl.inner.lock().expect("lock").boots.is_empty());
}

#[test]
fn boot_creates_then_boots() {
    let mut g = governor();
    let simctl = RecordingSimctl::default();
    let holding = g
        .boot(chan_a(), Some("glowmax"), None, None, &simctl)
        .expect("boot");
    assert_eq!(holding.lifecycle, DeviceLifecycle::Booted);
    assert!(holding.udid.is_some());
    assert_eq!(simctl.inner.lock().expect("lock").boots.len(), 1);
}

#[test]
fn cap_never_evicts_visible_mirror() {
    let mut g = governor();
    g.set_policy(GovernorPolicy {
        max_booted_sims: 1,
        ..GovernorPolicy::with_defaults()
    });
    let simctl = RecordingSimctl::default();
    g.boot(chan_a(), Some("A"), None, None, &simctl)
        .expect("boot A");
    g.set_pane_visible(chan_a(), true).expect("show A");
    let err = g
        .boot(chan_b(), Some("B"), None, None, &simctl)
        .expect_err("cap");
    assert!(err.contains("visible mirror") || err.contains("cap"));
    let a = g.sims.get(chan_a()).expect("A");
    assert!(a.holding.mirroring);
    assert_eq!(a.holding.lifecycle, DeviceLifecycle::Mirroring);
    assert!(simctl.inner.lock().expect("lock").shutdowns.is_empty());
}

#[test]
fn cap_proposes_idle_victim_not_mirror() {
    let mut g = governor();
    g.set_policy(GovernorPolicy {
        max_booted_sims: 2,
        ..GovernorPolicy::with_defaults()
    });
    let simctl = RecordingSimctl::default();
    g.boot(chan_a(), Some("A"), None, None, &simctl).expect("A");
    g.set_pane_visible(chan_a(), true).expect("show A");
    g.advance(60_000);
    g.boot(chan_b(), Some("B"), None, None, &simctl).expect("B");
    g.set_pane_visible(chan_b(), false).ok();
    g.advance(8 * 60_000);
    let chan_c = "cccccccc-cccc-cccc-cccc-cccccccccccc";
    let err = g
        .boot(chan_c, Some("C"), None, None, &simctl)
        .expect_err("cap");
    assert!(err.contains("cap"));
    let conflict = g.cap_conflict.expect("conflict");
    assert_eq!(conflict.victim_channel_id, chan_b());
    assert_ne!(conflict.victim_channel_id, chan_a());
    g.keep_sim(chan_b()).expect("keep B");
    assert!(g.cap_conflict.is_none());
}

#[test]
fn hidden_pane_stops_stream_after_two_seconds_device_stays_booted() {
    let mut g = governor();
    let simctl = RecordingSimctl::default();
    g.boot(chan_a(), Some("A"), None, None, &simctl)
        .expect("boot");
    g.set_pane_visible(chan_a(), true).expect("show");
    assert!(g.sims.get(chan_a()).expect("A").holding.mirroring);
    g.set_pane_visible(chan_a(), false).expect("hide");
    g.advance(1_999);
    assert!(g.sims.get(chan_a()).expect("A").holding.mirroring);
    g.advance(2);
    let rec = g.sims.get(chan_a()).expect("A");
    assert!(!rec.holding.mirroring);
    assert_eq!(rec.holding.lifecycle, DeviceLifecycle::Booted);
    assert!(simctl.inner.lock().expect("lock").shutdowns.is_empty());
}

#[test]
fn idle_timer_then_keep_resets() {
    let mut g = governor();
    let simctl = RecordingSimctl::default();
    g.boot(chan_a(), Some("A"), None, None, &simctl)
        .expect("boot");
    g.set_pane_visible(chan_a(), false).ok();
    let before = g.sims.get(chan_a()).expect("A").holding.idle_deadline_ms;
    g.advance(5 * 60_000);
    g.keep_sim(chan_a()).expect("keep");
    let after = g.sims.get(chan_a()).expect("A").holding.idle_deadline_ms;
    assert!(after > before);
    g.advance(15 * 60_000);
    g.force_idle_shutdowns(&simctl);
    assert_eq!(
        g.sims.get(chan_a()).expect("A").holding.lifecycle,
        DeviceLifecycle::Shutdown
    );
}

#[test]
fn reconcile_adopts_crew_only() {
    let mut g = governor();
    let simctl = RecordingSimctl::new(vec![
        ListedDevice {
            udid: "CREW-1".into(),
            name: "crew-9a1657ac".into(),
            state: "Booted".into(),
            is_available: true,
            runtime: "iOS 18".into(),
            device_type: "iPhone 16 Pro".into(),
        },
        ListedDevice {
            udid: "PHONE".into(),
            name: "iPhone 15".into(),
            state: "Booted".into(),
            is_available: true,
            runtime: "iOS 18".into(),
            device_type: "iPhone 15".into(),
        },
    ]);
    let listed = simctl.list_devices().expect("list");
    g.reconcile(&listed, &simctl);
    let crew = g
        .sims
        .values()
        .find(|r| r.holding.device_name == "crew-9a1657ac")
        .expect("crew");
    assert!(!crew.holding.foreign);
    assert_eq!(crew.holding.lifecycle, DeviceLifecycle::Booted);
    let foreign = g
        .sims
        .values()
        .find(|r| r.holding.device_name == "iPhone 15")
        .expect("foreign");
    assert!(foreign.holding.foreign);
    g.advance(15 * 60_000);
    g.force_idle_shutdowns(&simctl);
    let foreign = g
        .sims
        .values()
        .find(|r| r.holding.device_name == "iPhone 15")
        .expect("foreign still");
    assert_eq!(foreign.holding.lifecycle, DeviceLifecycle::Booted);
    assert!(simctl
        .inner
        .lock()
        .expect("lock")
        .shutdowns
        .iter()
        .all(|u| u != "PHONE"));
}

#[test]
fn quit_shuts_down_crew_booted_only() {
    let mut g = governor();
    let simctl = RecordingSimctl::new(vec![ListedDevice {
        udid: "PHONE".into(),
        name: "iPhone 15".into(),
        state: "Booted".into(),
        is_available: true,
        runtime: "iOS 18".into(),
        device_type: "iPhone 15".into(),
    }]);
    g.boot(chan_a(), Some("A"), None, None, &simctl)
        .expect("boot");
    g.reconcile(&simctl.list_devices().expect("list"), &simctl);
    g.quit_cleanup(&simctl).expect("quit");
    let a = g.sims.get(chan_a()).expect("A");
    assert_eq!(a.holding.lifecycle, DeviceLifecycle::Shutdown);
    let shutdowns = simctl.inner.lock().expect("lock").shutdowns.clone();
    assert!(shutdowns.iter().any(|u| u != "PHONE"));
    assert!(!shutdowns.iter().any(|u| u == "PHONE"));
}

#[test]
fn erase_and_delete_go_through_simctl() {
    let mut g = governor();
    let simctl = RecordingSimctl::default();
    g.boot(chan_a(), Some("A"), None, None, &simctl)
        .expect("boot");
    g.erase_sim(chan_a(), &simctl).expect("erase");
    assert_eq!(simctl.inner.lock().expect("lock").erases.len(), 1);
    g.delete_sim(chan_a(), &simctl).expect("delete");
    assert_eq!(simctl.inner.lock().expect("lock").deletes.len(), 1);
    assert!(!g.sims.contains_key(chan_a()));
}

#[test]
fn archive_erases_delete_removes() {
    let mut g = governor();
    let simctl = RecordingSimctl::default();
    g.boot(chan_a(), Some("A"), None, None, &simctl)
        .expect("boot");
    g.on_channel_archived(chan_a(), &simctl).expect("archive");
    assert_eq!(simctl.inner.lock().expect("lock").erases.len(), 1);
    g.on_channel_deleted(chan_a(), &simctl).expect("delete");
    assert!(!g.sims.contains_key(chan_a()));
}

#[test]
fn lru_skips_visible_mirror() {
    let mut g = governor();
    let simctl = RecordingSimctl::default();
    g.boot(chan_a(), Some("A"), None, None, &simctl).expect("A");
    g.set_pane_visible(chan_a(), true).ok();
    g.advance(10);
    g.boot(chan_b(), Some("B"), None, None, &simctl).expect("B");
    let victim = g.lru_sim_victim().expect("victim");
    assert_eq!(victim, chan_b());
}

#[test]
fn status_counts_match_holdings() {
    let mut g = governor();
    let simctl = RecordingSimctl::default();
    g.boot(chan_a(), Some("A"), None, None, &simctl).expect("A");
    g.set_pane_visible(chan_a(), true).ok();
    g.start_dev_server(
        chan_a(),
        "worktree",
        "pnpm dev --port $PORT",
        "/tmp/wt",
        "Local:",
        None,
    )
    .expect("server");
    let status = g.status();
    assert_eq!(status.booted_count, 1);
    assert_eq!(status.stream_count, 1);
    assert_eq!(status.server_count, 1);
    assert_eq!(status.sims.len(), 1);
    assert_eq!(status.servers.len(), 1);
}
