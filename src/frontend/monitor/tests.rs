use super::*;
use crate::frontend::events::LibraryEvent;
use crate::frontend::fakes::{FakeMediaControls, FakeMemoryProbe, FakePlayer, FakeSystemAudio};
use crate::frontend::ports::{ReplayGains, TrackRequest};
use crate::frontend::threading::test_support::{TEST_TIMEOUT, with_timeout};
use crate::integrations::control::external_control_channel;
use std::sync::mpsc;

const T0: u64 = 1_000_000;

fn config() -> MonitorConfig {
    MonitorConfig {
        autosave_interval: Some(Duration::from_secs(20)),
    }
}

fn playing() -> AudioState {
    AudioState {
        is_playing: true,
        path: "a.flac".to_string(),
        title: "A".to_string(),
        artist: "Artist".to_string(),
        album: "Album".to_string(),
        total_duration_sec: 200.0,
        current_pos_sec: 10.0,
        ..AudioState::default()
    }
}

fn paused() -> AudioState {
    AudioState {
        is_playing: false,
        ..playing()
    }
}

fn inputs(audio: AudioState, unix_secs: u64) -> MonitorInputs {
    MonitorInputs {
        audio,
        unix_secs,
        external: Vec::new(),
        library_dirty: false,
        is_scanning: false,
        preload: PreloadSnapshot::default(),
        memory: None,
    }
}

fn quiet() -> MemorySample {
    MemorySample::default()
}

fn with_memory(audio: AudioState, unix_secs: u64, sample: MemorySample) -> MonitorInputs {
    MonitorInputs {
        memory: Some(sample),
        ..inputs(audio, unix_secs)
    }
}

fn label(event: &Event) -> String {
    match event {
        Event::Playback(PlaybackEvent::TrackChanged(_)) => "TrackChanged".into(),
        Event::Playback(PlaybackEvent::StateChanged(_)) => "StateChanged".into(),
        Event::Playback(PlaybackEvent::PositionChanged { .. }) => "PositionChanged".into(),
        Event::Playback(PlaybackEvent::DeviceChanged(_)) => "DeviceChanged".into(),
        Event::Playback(PlaybackEvent::NoticeChanged(_)) => "NoticeChanged".into(),
        Event::Playback(PlaybackEvent::EndOfTrack(_)) => "EndOfTrack".into(),
        Event::Library(event) => format!("Library::{event:?}"),
        Event::Session(SessionEvent::AutosaveDue) => "AutosaveDue".into(),
        Event::Maintenance(MaintenanceEvent::PurgeRequested(reason)) => {
            format!("Purge({reason:?})")
        }
        Event::ExternalControl(event) => format!("External({event:?})"),
        other => format!("{other:?}"),
    }
}

fn labels(output: &PollOutput) -> Vec<String> {
    output.events.iter().map(label).collect()
}

fn purges(output: &PollOutput) -> Vec<String> {
    labels(output)
        .into_iter()
        .filter(|label| label.starts_with("Purge"))
        .collect()
}

fn media_updates(output: &PollOutput) -> usize {
    output
        .actions
        .iter()
        .filter(|action| **action == MonitorAction::UpdateMediaControls)
        .count()
}

/// A monitor that has already seen `audio`, so the next poll shows only diffs.
fn primed(audio: &AudioState, now: Instant) -> MonitorState {
    let mut state = MonitorState::new(config(), T0);
    poll_once(&mut state, now, &inputs(audio.clone(), T0));
    state
}

#[test]
fn the_first_poll_publishes_the_whole_state_and_later_polls_only_the_diffs() {
    let t0 = Instant::now();
    let mut state = MonitorState::new(config(), T0);
    let first = poll_once(&mut state, t0, &inputs(playing(), T0));
    assert_eq!(
        labels(&first),
        [
            "TrackChanged",
            "StateChanged",
            "PositionChanged",
            "DeviceChanged",
            "NoticeChanged"
        ]
    );

    let unchanged = poll_once(&mut state, t0 + POLL_WHILE_PLAYING, &inputs(playing(), T0));
    assert!(unchanged.events.is_empty());

    let moved = AudioState {
        current_pos_sec: 10.1,
        ..playing()
    };
    let output = poll_once(
        &mut state,
        t0 + 2 * POLL_WHILE_PLAYING,
        &inputs(moved.clone(), T0),
    );
    assert_eq!(labels(&output), ["PositionChanged"]);

    let louder = AudioState {
        volume: 0.8,
        ..moved.clone()
    };
    let output = poll_once(
        &mut state,
        t0 + 3 * POLL_WHILE_PLAYING,
        &inputs(louder.clone(), T0),
    );
    assert_eq!(labels(&output), ["StateChanged"]);

    let with_notice = AudioState {
        audio_notice: Some("device missing".to_string()),
        device_sample_rate: 96_000,
        ..louder
    };
    let output = poll_once(
        &mut state,
        t0 + 4 * POLL_WHILE_PLAYING,
        &inputs(with_notice, T0),
    );
    assert_eq!(labels(&output), ["DeviceChanged", "NoticeChanged"]);
}

#[test]
fn a_new_track_publishes_track_changed_with_the_new_metadata() {
    let t0 = Instant::now();
    let mut state = primed(&playing(), t0);
    let next = AudioState {
        path: "b.flac".to_string(),
        title: "B".to_string(),
        ..playing()
    };
    let output = poll_once(&mut state, t0 + POLL_WHILE_PLAYING, &inputs(next, T0));
    assert_eq!(labels(&output), ["TrackChanged"]);
    match &output.events[0] {
        Event::Playback(PlaybackEvent::TrackChanged(snapshot)) => {
            assert_eq!(snapshot.title, "B");
            assert_eq!(snapshot.path, "b.flac");
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn the_poll_interval_is_short_while_playing_and_long_when_idle() {
    let t0 = Instant::now();
    let mut state = MonitorState::new(config(), T0);
    let output = poll_once(&mut state, t0, &inputs(playing(), T0));
    assert_eq!(output.next_poll_in, POLL_WHILE_PLAYING);
    let output = poll_once(&mut state, t0 + POLL_WHILE_PLAYING, &inputs(paused(), T0));
    assert_eq!(output.next_poll_in, POLL_WHILE_IDLE);
    assert_eq!(POLL_WHILE_PLAYING, Duration::from_millis(100));
    assert_eq!(POLL_WHILE_IDLE, Duration::from_millis(500));
}

#[test]
fn end_of_track_is_published_once_while_the_flag_stays_set() {
    let t0 = Instant::now();
    let mut state = primed(&playing(), t0);
    let ended = AudioState {
        eof_reached: true,
        ..playing()
    };
    let output = poll_once(
        &mut state,
        t0 + POLL_WHILE_PLAYING,
        &inputs(ended.clone(), T0),
    );
    assert_eq!(labels(&output), ["EndOfTrack"]);
    match &output.events[0] {
        Event::Playback(PlaybackEvent::EndOfTrack(snapshot)) => assert!(snapshot.eof_reached),
        other => panic!("unexpected {other:?}"),
    }

    // Nobody took the flag: no second event, and nothing in the output can
    // clear it (the monitor only reads the state).
    let output = poll_once(&mut state, t0 + 2 * POLL_WHILE_PLAYING, &inputs(ended, T0));
    assert!(output.events.is_empty());
}

#[test]
fn a_second_end_of_track_on_another_track_is_published_even_if_the_flag_never_dropped() {
    let t0 = Instant::now();
    let ended = AudioState {
        eof_reached: true,
        ..playing()
    };
    let mut state = primed(&ended, t0);
    let promoted_and_ended = AudioState {
        path: "b.flac".to_string(),
        ..ended
    };
    let output = poll_once(
        &mut state,
        t0 + POLL_WHILE_PLAYING,
        &inputs(promoted_and_ended, T0),
    );
    assert_eq!(labels(&output), ["TrackChanged", "EndOfTrack"]);
}

#[test]
fn the_media_controls_are_updated_only_when_what_they_show_changes() {
    let t0 = Instant::now();
    let mut state = MonitorState::new(config(), T0);
    let at = |secs: f64| AudioState {
        current_pos_sec: secs,
        ..playing()
    };
    let step = POLL_WHILE_PLAYING;
    assert_eq!(
        media_updates(&poll_once(&mut state, t0, &inputs(at(10.0), T0))),
        1
    );
    assert_eq!(
        media_updates(&poll_once(&mut state, t0 + step, &inputs(at(10.4), T0))),
        0
    );
    assert_eq!(
        media_updates(&poll_once(&mut state, t0 + 2 * step, &inputs(at(10.9), T0))),
        0
    );
    assert_eq!(
        media_updates(&poll_once(&mut state, t0 + 3 * step, &inputs(at(11.0), T0))),
        1
    );
    let paused_at = AudioState {
        is_playing: false,
        ..at(11.0)
    };
    assert_eq!(
        media_updates(&poll_once(
            &mut state,
            t0 + 4 * step,
            &inputs(paused_at.clone(), T0)
        )),
        1
    );
    assert_eq!(
        media_updates(&poll_once(
            &mut state,
            t0 + 5 * step,
            &inputs(paused_at, T0)
        )),
        0
    );
    let other_track = AudioState {
        title: "C".to_string(),
        ..at(11.0)
    };
    assert_eq!(
        media_updates(&poll_once(
            &mut state,
            t0 + 6 * step,
            &inputs(other_track, T0)
        )),
        1
    );
}

#[test]
fn autosave_is_due_every_interval_while_playing() {
    let t0 = Instant::now();
    let mut state = MonitorState::new(config(), T0);
    let mut autosaves_at = |offset_secs: u64, audio: AudioState| {
        let output = poll_once(
            &mut state,
            t0 + Duration::from_secs(offset_secs),
            &inputs(audio, T0),
        );
        labels(&output).contains(&"AutosaveDue".to_string())
    };
    assert!(!autosaves_at(0, playing()));
    assert!(!autosaves_at(19, playing()));
    assert!(autosaves_at(20, playing()));
    assert!(!autosaves_at(39, playing()));
    assert!(autosaves_at(40, playing()));
}

#[test]
fn no_autosave_is_due_while_paused_and_resuming_restarts_the_interval() {
    let t0 = Instant::now();
    let mut state = MonitorState::new(config(), T0);
    let mut autosaves_at = |offset_secs: u64, audio: AudioState| {
        let output = poll_once(
            &mut state,
            t0 + Duration::from_secs(offset_secs),
            &inputs(audio, T0),
        );
        labels(&output).contains(&"AutosaveDue".to_string())
    };
    assert!(!autosaves_at(0, playing()));
    assert!(!autosaves_at(15, paused()));
    assert!(!autosaves_at(60, paused()));
    assert!(!autosaves_at(100, paused()));
    assert!(!autosaves_at(101, playing()));
    assert!(!autosaves_at(120, playing()));
    assert!(autosaves_at(121, playing()));
}

#[test]
fn an_interval_of_zero_disables_the_autosave() {
    let mut app = AppConfig::default();
    app.behavior.auto_save_interval_secs = 0;
    let config = MonitorConfig::from_app_config(&app);
    assert_eq!(config.autosave_interval, None);
    let t0 = Instant::now();
    let mut state = MonitorState::new(config, T0);
    for secs in [0, 20, 40, 60] {
        let output = poll_once(
            &mut state,
            t0 + Duration::from_secs(secs),
            &inputs(playing(), T0),
        );
        assert!(!labels(&output).contains(&"AutosaveDue".to_string()));
    }
    assert_eq!(
        MonitorConfig::from_app_config(&AppConfig::default()).autosave_interval,
        Some(Duration::from_secs(20))
    );
}

#[test]
fn the_ram_hard_cap_purges_at_once_even_during_a_scan_or_a_preload() {
    let t0 = Instant::now();
    let mut state = MonitorState::new(config(), T0);
    let mut busy = with_memory(
        playing(),
        T0 + 1,
        MemorySample {
            system_over_hard_cap: true,
            ..quiet()
        },
    );
    busy.is_scanning = true;
    busy.preload.next_path = Some("next.flac".to_string());
    let output = poll_once(&mut state, t0, &busy);
    assert_eq!(purges(&output), ["Purge(RamHardCap)"]);
    assert!(output.actions.contains(&MonitorAction::BackendPurge));
}

#[test]
fn d23_the_ram_hard_cap_purges_at_most_every_ten_seconds() {
    let t0 = Instant::now();
    let mut state = MonitorState::new(config(), T0);
    let over = MemorySample {
        system_over_hard_cap: true,
        ..quiet()
    };
    let at = |secs: u64| with_memory(playing(), T0 + secs, over.clone());
    // Iced purged on every tick while over the cap.
    assert_eq!(
        purges(&poll_once(&mut state, t0, &at(1))),
        ["Purge(RamHardCap)"]
    );
    for secs in 2..11 {
        let output = poll_once(&mut state, t0, &at(secs));
        assert!(
            purges(&output).is_empty(),
            "still in the cooldown at {secs} s"
        );
        assert!(!output.actions.contains(&MonitorAction::BackendPurge));
    }
    assert_eq!(
        purges(&poll_once(&mut state, t0, &at(11))),
        ["Purge(RamHardCap)"]
    );
}

#[test]
fn the_ram_hard_cap_and_the_player_ram_limit_share_one_cooldown() {
    let t0 = Instant::now();
    let mut state = MonitorState::new(config(), T0);
    let hard_cap = MemorySample {
        system_over_hard_cap: true,
        ..quiet()
    };
    let app_over = MemorySample {
        app_over_limit: true,
        ..quiet()
    };
    assert_eq!(
        purges(&poll_once(
            &mut state,
            t0,
            &with_memory(paused(), T0 + 1, hard_cap)
        )),
        ["Purge(RamHardCap)"]
    );
    assert!(
        purges(&poll_once(
            &mut state,
            t0,
            &with_memory(paused(), T0 + 6, app_over.clone())
        ))
        .is_empty()
    );
    assert_eq!(
        purges(&poll_once(
            &mut state,
            t0,
            &with_memory(paused(), T0 + 11, app_over)
        )),
        ["Purge(RamLimit)"]
    );
}

#[test]
fn the_backend_purge_forgets_the_preload_it_dropped() {
    with_timeout(TEST_TIMEOUT, || {
        let player = Arc::new(FakePlayer::new());
        let engine = EngineWorker::new(player.clone(), Arc::new(FakeSystemAudio::new())).unwrap();
        let preload = Arc::new(PreloadState::new());
        let track = TrackRequest {
            path: "b.flac".to_string(),
            ..TrackRequest::default()
        };
        player.preload_next(&track, ReplayGains::default()).unwrap();
        preload.mark_preloaded("b.flac");

        queue_backend_purge(&engine, &preload)
            .recv_blocking()
            .unwrap();
        engine.run(|_| Ok(())).recv_blocking().unwrap();

        assert_eq!(player.preloaded_path(), None);
        assert_eq!(
            preload.preloaded_next_path(),
            None,
            "the next track gets preloaded again"
        );

        // Without a pending preload, the 20 s window after a track change stays.
        preload.mark_track_advanced(T0);
        queue_backend_purge(&engine, &preload)
            .recv_blocking()
            .unwrap();
        engine.run(|_| Ok(())).recv_blocking().unwrap();
        assert_eq!(preload.finished_at(), Some(T0));
        assert!(engine.shutdown(TEST_TIMEOUT));
    });
}

#[test]
fn the_timer_purge_runs_every_sixty_seconds() {
    let t0 = Instant::now();
    let mut state = MonitorState::new(config(), T0);
    let output = poll_once(&mut state, t0, &with_memory(paused(), T0 + 59, quiet()));
    assert!(purges(&output).is_empty());
    assert!(!output.actions.contains(&MonitorAction::BackendPurge));
    let output = poll_once(&mut state, t0, &with_memory(paused(), T0 + 60, quiet()));
    assert_eq!(purges(&output), ["Purge(Timer)"]);
    assert!(output.actions.contains(&MonitorAction::BackendPurge));
    let output = poll_once(&mut state, t0, &with_memory(paused(), T0 + 61, quiet()));
    assert!(purges(&output).is_empty(), "the timer restarted");
}

#[test]
fn the_timer_purge_waits_while_a_scan_runs() {
    let t0 = Instant::now();
    let mut state = MonitorState::new(config(), T0);
    let mut scanning = with_memory(paused(), T0 + 60, quiet());
    scanning.is_scanning = true;
    assert!(purges(&poll_once(&mut state, t0, &scanning)).is_empty());
    let output = poll_once(&mut state, t0, &with_memory(paused(), T0 + 61, quiet()));
    assert_eq!(purges(&output), ["Purge(Timer)"]);
}

#[test]
fn the_timer_purge_waits_for_the_preload_safety_window() {
    let t0 = Instant::now();
    let mut state = MonitorState::new(config(), T0);
    let mut preloading = with_memory(playing(), T0 + 60, quiet());
    preloading.preload.next_path = Some("next.flac".to_string());
    assert!(purges(&poll_once(&mut state, t0, &preloading)).is_empty());

    let mut just_advanced = with_memory(playing(), T0 + 70, quiet());
    just_advanced.preload.finished_at = Some(T0 + 55);
    assert!(purges(&poll_once(&mut state, t0, &just_advanced)).is_empty());

    let mut window_over = with_memory(playing(), T0 + 75, quiet());
    window_over.preload.finished_at = Some(T0 + 55);
    assert_eq!(
        purges(&poll_once(&mut state, t0, &window_over)),
        ["Purge(Timer)"]
    );
}

#[test]
fn the_player_ram_limit_purges_at_most_every_ten_seconds() {
    let t0 = Instant::now();
    let mut state = MonitorState::new(config(), T0);
    let over = MemorySample {
        app_over_limit: true,
        process_ram_bytes: 600 * 1024 * 1024,
        ..quiet()
    };
    let output = poll_once(&mut state, t0, &with_memory(paused(), T0 + 5, over.clone()));
    assert_eq!(purges(&output), ["Purge(RamLimit)"]);
    let output = poll_once(
        &mut state,
        t0,
        &with_memory(paused(), T0 + 14, over.clone()),
    );
    assert!(purges(&output).is_empty(), "still in the cooldown");
    let output = poll_once(&mut state, t0, &with_memory(paused(), T0 + 15, over));
    assert_eq!(purges(&output), ["Purge(RamLimit)"]);
}

#[test]
fn a_purge_runs_forty_seconds_after_a_scan_ends() {
    let t0 = Instant::now();
    let mut state = MonitorState::new(config(), T0);
    let mut scanning = inputs(paused(), T0);
    scanning.is_scanning = true;
    let output = poll_once(&mut state, t0, &scanning);
    assert!(labels(&output).contains(&"Library::ScanStarted".to_string()));
    let output = poll_once(&mut state, t0, &inputs(paused(), T0 + 1));
    assert!(labels(&output).contains(&"Library::ScanFinished".to_string()));

    let output = poll_once(&mut state, t0, &with_memory(paused(), T0 + 40, quiet()));
    assert!(purges(&output).is_empty());
    let output = poll_once(&mut state, t0, &with_memory(paused(), T0 + 41, quiet()));
    assert_eq!(purges(&output), ["Purge(ScanFinished)"]);
    let output = poll_once(&mut state, t0, &with_memory(paused(), T0 + 61, quiet()));
    assert!(
        purges(&output).is_empty(),
        "the scan purge restarted the 60 s timer"
    );
}

#[test]
fn housekeeping_runs_only_with_a_memory_sample_and_logs_the_metrics() {
    let t0 = Instant::now();
    let mut state = MonitorState::new(config(), T0);
    let output = poll_once(&mut state, t0, &inputs(paused(), T0 + 3600));
    assert!(purges(&output).is_empty(), "no sample, no housekeeping");
    assert!(
        output
            .actions
            .iter()
            .all(|a| !matches!(a, MonitorAction::LogMetrics(_)))
    );

    let sample = MemorySample {
        process_ram_bytes: 123,
        underruns: 2,
        latency_peak_us: 900,
        ..quiet()
    };
    let output = poll_once(
        &mut state,
        t0,
        &with_memory(paused(), T0 + 1, sample.clone()),
    );
    assert!(output.actions.contains(&MonitorAction::LogMetrics(sample)));
}

#[test]
fn housekeeping_follows_the_tick_cadence_of_the_original_ui() {
    let t0 = Instant::now();
    let mut state = MonitorState::new(config(), T0);
    assert!(state.housekeeping_due(t0, false), "the first poll samples");
    poll_once(&mut state, t0, &with_memory(paused(), T0, quiet()));
    assert!(!state.housekeeping_due(t0 + Duration::from_millis(499), true));
    assert!(state.housekeeping_due(t0 + HOUSEKEEPING_WHILE_BUSY, true));
    assert!(!state.housekeeping_due(t0 + Duration::from_millis(4_999), false));
    assert!(state.housekeeping_due(t0 + HOUSEKEEPING_WHILE_IDLE, false));
}

#[test]
fn external_control_events_are_forwarded_in_order() {
    let t0 = Instant::now();
    let mut state = primed(&paused(), t0);
    let mut received = inputs(paused(), T0);
    received.external = vec![
        ExternalControlEvent::Play,
        ExternalControlEvent::SeekTo(3.0),
        ExternalControlEvent::Next,
    ];
    let output = poll_once(&mut state, t0, &received);
    assert_eq!(
        labels(&output),
        ["External(Play)", "External(SeekTo(3.0))", "External(Next)"]
    );
}

#[test]
fn the_scanner_flags_become_library_events() {
    let t0 = Instant::now();
    let mut state = primed(&paused(), t0);
    let mut dirty = inputs(paused(), T0);
    dirty.library_dirty = true;
    dirty.is_scanning = true;
    assert_eq!(
        labels(&poll_once(&mut state, t0, &dirty)),
        ["Library::ScanStarted", "Library::Changed"]
    );
    let mut still = inputs(paused(), T0);
    still.is_scanning = true;
    assert!(poll_once(&mut state, t0, &still).events.is_empty());
    assert_eq!(
        labels(&poll_once(&mut state, t0, &inputs(paused(), T0))),
        ["Library::ScanFinished"]
    );
}

fn wait_for(rx: &mpsc::Receiver<Event>, what: &str, matches: impl Fn(&Event) -> bool) -> Event {
    let deadline = Instant::now() + TEST_TIMEOUT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(event) if matches(&event) => return event,
            Ok(_) => {}
            Err(_) => panic!("no {what} event"),
        }
    }
}

#[test]
fn the_monitor_thread_publishes_changes_and_leaves_the_eof_flag_to_the_ui() {
    with_timeout(TEST_TIMEOUT, || {
        let player = Arc::new(FakePlayer::new());
        let media = Arc::new(FakeMediaControls::new());
        let hub = EventHub::new().unwrap();
        let engine = EngineWorker::new(player.clone(), Arc::new(FakeSystemAudio::new())).unwrap();
        let (external_tx, external_rx) = external_control_channel();
        let db_dirty = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let _token = hub.subscribe_all(Arc::new(move |event| {
            let _ = tx.lock().send(event);
        }));

        let monitor = PlaybackMonitor::spawn(MonitorDeps {
            player: player.clone(),
            media: media.clone(),
            hub: hub.clone(),
            engine: engine.clone(),
            preload: Arc::new(PreloadState::new()),
            db_dirty: Arc::clone(&db_dirty),
            is_scanning: Arc::new(AtomicBool::new(false)),
            external: external_rx,
            probe: Arc::new(FakeMemoryProbe::new()),
            config: config(),
        })
        .unwrap();

        let track = |path: &str| TrackRequest {
            path: path.to_string(),
            title: path.to_uppercase(),
            ..TrackRequest::default()
        };
        player
            .load_file(&track("a.flac"), ReplayGains::default())
            .unwrap();
        player.play();
        player
            .preload_next(&track("b.flac"), ReplayGains::default())
            .unwrap();
        wait_for(
            &rx,
            "TrackChanged(A.FLAC)",
            |event| matches!(event, Event::Playback(PlaybackEvent::TrackChanged(s)) if s.title == "A.FLAC"),
        );

        external_tx.send(ExternalControlEvent::Next);
        wait_for(&rx, "ExternalControl(Next)", |event| {
            matches!(event, Event::ExternalControl(ExternalControlEvent::Next))
        });

        player.finish_track();
        wait_for(
            &rx,
            "EndOfTrack",
            |event| matches!(event, Event::Playback(PlaybackEvent::EndOfTrack(s)) if s.path == "b.flac"),
        );
        std::thread::sleep(3 * POLL_WHILE_PLAYING);
        assert!(
            player.snapshot().eof_reached,
            "the monitor never clears the flag"
        );
        assert!(player.take_eof());
        assert!(!player.take_eof());

        db_dirty.store(true, Ordering::Relaxed);
        wait_for(&rx, "Library::Changed", |event| {
            matches!(event, Event::Library(LibraryEvent::Changed))
        });
        assert!(
            !db_dirty.load(Ordering::Relaxed),
            "the monitor took the flag"
        );
        assert!(media.update_count() >= 1);

        assert!(monitor.shutdown(TEST_TIMEOUT));
        assert!(monitor.is_stopped());
        assert!(engine.shutdown(TEST_TIMEOUT));
        assert!(hub.shutdown(TEST_TIMEOUT));
    });
}
