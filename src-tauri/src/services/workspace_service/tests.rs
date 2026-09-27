use super::*;
use std::fs;

fn mount(service: &WorkspaceService, path: &std::path::Path) -> WorkspaceRoot {
    service
        .mount(
            ResourceRef::DesktopDirectory {
                path: path.to_str().unwrap().into(),
            },
            Arc::new(|_| {}),
        )
        .unwrap()
}

#[test]
fn workspace_pages_and_refresh_reject_old_cursors_and_roots() {
    let fixture = tempfile::tempdir().unwrap();
    for name in ["b.md", "a.md", "c.md"] {
        fs::write(fixture.path().join(name), "body").unwrap();
    }
    let service = WorkspaceService::default();
    let root = mount(&service, fixture.path());
    service.cancel(&root.root_id, "not-started").unwrap();
    assert_eq!(
        service
            .page(&root.root_id, "", None, "not-started", 2)
            .unwrap_err()
            .code,
        "WORKSPACE_CANCELLED"
    );
    let first = service.page(&root.root_id, "", None, "first", 2).unwrap();
    assert_eq!(
        first
            .entries
            .iter()
            .map(|e| e.name.as_str())
            .collect::<Vec<_>>(),
        ["a.md", "b.md"]
    );
    let last = service
        .page(&root.root_id, "", first.next.clone(), "last", 2)
        .unwrap();
    assert_eq!(last.entries[0].name, "c.md");
    assert!(last.next.is_none());
    service.refresh(&root.root_id, "").unwrap();
    assert_eq!(
        service
            .page(&root.root_id, "", first.next, "stale", 2)
            .unwrap_err()
            .code,
        "WORKSPACE_STALE"
    );
    let second = mount(&service, fixture.path());
    assert_ne!(root.root_id, second.root_id);
    assert_eq!(
        service
            .page(&root.root_id, "", None, "old", 2)
            .unwrap_err()
            .code,
        "WORKSPACE_STALE"
    );
    assert_eq!(
        service.unmount(&root.root_id).unwrap_err().code,
        "WORKSPACE_STALE"
    );
    service.unmount(&second.root_id).unwrap();
}

#[test]
fn workspace_collapse_invalidates_snapshot_and_stops_expanded_watch() {
    let fixture = tempfile::tempdir().unwrap();
    fs::create_dir(fixture.path().join("child")).unwrap();
    fs::write(fixture.path().join("child/a.md"), "one").unwrap();
    fs::write(fixture.path().join("child/b.md"), "two").unwrap();
    let service = WorkspaceService::default();
    let root = mount(&service, fixture.path());
    let first = service
        .page(&root.root_id, "child", None, "first", 1)
        .unwrap();
    service.collapse(&root.root_id, "child").unwrap();
    assert!(service
        .root(&root.root_id)
        .unwrap()
        .inner
        .lock()
        .unwrap()
        .nodes
        .is_empty());
    assert_eq!(
        service
            .page(&root.root_id, "child", first.next, "old", 1)
            .unwrap_err()
            .code,
        "WORKSPACE_STALE"
    );
}

#[test]
fn workspace_real_watch_invalidates_only_expanded_directory() {
    use std::{sync::mpsc, time::Duration};
    let fixture = tempfile::tempdir().unwrap();
    fs::create_dir(fixture.path().join("child")).unwrap();
    fs::write(fixture.path().join("a.md"), "before").unwrap();
    let (sender, receiver) = mpsc::channel();
    let service = WorkspaceService::default();
    let root = service
        .mount(
            ResourceRef::DesktopDirectory {
                path: fixture.path().to_str().unwrap().into(),
            },
            Arc::new(move |event| {
                let _ = sender.send(event);
            }),
        )
        .unwrap();
    let first = service.page(&root.root_id, "", None, "first", 1).unwrap();
    assert!(first.watch_issue.is_none());
    fs::write(fixture.path().join("child/inside.md"), "unexpanded").unwrap();
    assert!(receiver.recv_timeout(Duration::from_millis(350)).is_err());
    fs::write(fixture.path().join("b.md"), "new").unwrap();
    let event = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(event.root_id, root.root_id);
    assert_eq!(event.directories.len(), 1);
    assert_eq!(event.directories[0].relative_path, "");
    assert!(event.directories[0].generation > first.generation);
    assert_eq!(
        service
            .page(&root.root_id, "", first.next, "old", 1)
            .unwrap_err()
            .code,
        "WORKSPACE_STALE"
    );
    service
        .page(&root.root_id, "child", None, "child", 10)
        .unwrap();
    fs::remove_file(fixture.path().join("child/inside.md")).unwrap();
    fs::remove_dir(fixture.path().join("child")).unwrap();
    assert_eq!(
        service
            .page(&root.root_id, "child", None, "deleted", 10)
            .unwrap_err()
            .code,
        "WORKSPACE_NOT_FOUND"
    );
}

#[test]
fn workspace_overflow_and_late_old_root_event_are_explicit() {
    use crate::platform::desktop::workspace_watch::ChangeBatch;
    use std::collections::BTreeSet;
    let fixture = tempfile::tempdir().unwrap();
    fs::write(fixture.path().join("a.md"), "").unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = events.clone();
    let service = WorkspaceService::default();
    let dto = service
        .mount(
            ResourceRef::DesktopDirectory {
                path: fixture.path().to_str().unwrap().into(),
            },
            Arc::new(move |event| captured.lock().unwrap().push(event)),
        )
        .unwrap();
    let first = service.page(&dto.root_id, "", None, "one", 1).unwrap();
    let root = service.root(&dto.root_id).unwrap();
    root.invalidate(ChangeBatch {
        paths: BTreeSet::new(),
        overflow: true,
    });
    assert!(events.lock().unwrap()[0].overflow);
    assert_eq!(
        service
            .page(
                &dto.root_id,
                "",
                Some(PageCursor {
                    generation: first.generation,
                    offset: 0
                }),
                "old",
                1
            )
            .unwrap_err()
            .code,
        "WORKSPACE_STALE"
    );
    mount(&service, fixture.path());
    let count = events.lock().unwrap().len();
    root.invalidate(ChangeBatch {
        paths: BTreeSet::new(),
        overflow: true,
    });
    assert_eq!(events.lock().unwrap().len(), count);
}

#[test]
fn workspace_file_resolution_reuses_existing_file_identity() {
    let fixture = tempfile::tempdir().unwrap();
    let source = fixture.path().join("a.md");
    fs::write(&source, "# one file").unwrap();
    fs::hard_link(&source, fixture.path().join("alias.md")).unwrap();
    let service = WorkspaceService::default();
    let root = mount(&service, fixture.path());
    let tree = crate::services::file_service::read_resource(
        &service.resolve(&root.root_id, "alias.md").unwrap(),
    )
    .unwrap();
    let direct =
        crate::services::file_service::read_markdown_file(source.to_str().unwrap()).unwrap();
    assert_eq!(tree.file_identity, direct.file_identity);
    assert_eq!(tree.content, direct.content);
    assert!(service.resolve(&root.root_id, "../outside.md").is_err());
    assert!(service.resolve(&root.root_id, "/outside.md").is_err());
}

#[test]
fn workspace_replaced_root_is_not_silently_reauthorized() {
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture.path().join("root");
    fs::create_dir(&path).unwrap();
    let service = WorkspaceService::default();
    let dto = mount(&service, &path);
    fs::rename(&path, fixture.path().join("original")).unwrap();
    fs::create_dir(&path).unwrap();
    assert_eq!(
        service
            .page(&dto.root_id, "", None, "replaced", 10)
            .unwrap_err()
            .code,
        "WORKSPACE_STALE"
    );
}

#[cfg(windows)]
#[test]
fn workspace_permission_denial_is_reported_for_only_the_selected_node() {
    use std::process::Command;
    struct RestoreAcl(std::path::PathBuf);
    impl Drop for RestoreAcl {
        fn drop(&mut self) {
            let _ = Command::new("icacls")
                .arg(&self.0)
                .args(["/remove:d", "*S-1-1-0"])
                .output();
        }
    }
    let fixture = tempfile::tempdir().unwrap();
    let denied = fixture.path().join("denied");
    fs::create_dir(&denied).unwrap();
    fs::write(fixture.path().join("readable.md"), "safe").unwrap();
    let service = WorkspaceService::default();
    let root = mount(&service, fixture.path());
    let _restore = RestoreAcl(denied.clone());
    assert!(Command::new("icacls")
        .arg(&denied)
        .args(["/deny", "*S-1-1-0:(RD)"])
        .output()
        .unwrap()
        .status
        .success());
    let page = service.page(&root.root_id, "", None, "root", 10).unwrap();
    assert!(page.entries.iter().any(|entry| entry.name == "readable.md"));
    assert_eq!(
        service
            .page(&root.root_id, "denied", None, "denied", 10)
            .unwrap_err()
            .code,
        "WORKSPACE_PERMISSION_DENIED"
    );
}

#[test]
#[ignore = "Creates and enumerates 100,000 real files; run explicitly for directory acceptance"]
fn workspace_hundred_thousand_entries_are_paged_and_cancellable() {
    use std::{
        thread,
        time::{Duration, Instant},
    };
    let fixture = tempfile::tempdir().unwrap();
    for index in 0..100_000 {
        fs::File::create(fixture.path().join(format!("{index:06}.md"))).unwrap();
    }
    let service = Arc::new(WorkspaceService::default());
    let dto = mount(&service, fixture.path());
    let root = service.root(&dto.root_id).unwrap();
    let task_service = service.clone();
    let task_id = dto.root_id.clone();
    let pending = thread::spawn(move || task_service.page(&task_id, "", None, "cancel-me", 256));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.inner.lock().unwrap().jobs.contains_key("cancel-me") {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    service.cancel(&dto.root_id, "cancel-me").unwrap();
    assert_eq!(
        pending.join().unwrap().unwrap_err().code,
        "WORKSPACE_CANCELLED"
    );
    let start = Instant::now();
    let first = service.page(&dto.root_id, "", None, "first", 256).unwrap();
    assert_eq!(first.entries.len(), 256);
    assert_eq!(first.entries[0].name, "000000.md");
    let first_ms = start.elapsed().as_millis();
    let mut count = first.entries.len();
    let mut cursor = first.next;
    let mut previous = first.entries.last().unwrap().name.clone();
    while let Some(next) = cursor {
        let page = service
            .page(&dto.root_id, "", Some(next), "next", 256)
            .unwrap();
        assert!(page.entries.len() <= 256);
        for entry in &page.entries {
            assert!(entry.name > previous);
            previous = entry.name.clone();
        }
        count += page.entries.len();
        cursor = page.next;
    }
    assert_eq!(count, 100_000);
    assert_eq!(previous, "099999.md");
    let bytes = root.inner.lock().unwrap().nodes[""]
        .snapshot
        .as_ref()
        .unwrap()
        .bytes;
    assert!(bytes <= crate::platform::desktop::workspace::SNAPSHOT_BYTES);
    println!(
        "100k directory: first page {first_ms}ms, all pages {}ms, accounted snapshot {bytes} bytes",
        start.elapsed().as_millis()
    );
    service.unmount(&dto.root_id).unwrap();
}
