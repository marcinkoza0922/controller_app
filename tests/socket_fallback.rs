//! Without `XDG_RUNTIME_DIR` the daemon's control socket lives in a per-user directory under the
//! temp dir. Anyone who can send it a request can drive the daemon, so that directory must be
//! private (0700, owned by the daemon's user), the socket must be 0600, and another user must be
//! refused. Runs as root, since the other user is made with `setpriv`:
//! `cargo test --test socket_fallback -- --ignored`, or `scripts/kernel-test-docker.sh socket_fallback`.

mod common;

use std::{
    fs,
    os::unix::{
        fs::{MetadataExt, PermissionsExt, chown},
        net::UnixStream,
    },
    path::{Path, PathBuf},
    process::Command,
};

use common::{daemon_log, serial, spawn_daemon, temp_root, wait_for};

/// An unprivileged account on Debian and Ubuntu images.
const OTHER_UID: u32 = 65534;

fn own_uid() -> u32 {
    // SAFETY: getuid cannot fail.
    unsafe { libc::getuid() }
}

fn socket_in(root: &Path) -> PathBuf {
    root.join("tmp").join(format!("padwight-{}", own_uid())).join("padwight.sock")
}

/// Runs `sh -c script` as the other user, with `arg` as its first argument.
fn as_other_user(script: &str, arg: &Path) -> std::process::Output {
    Command::new("setpriv")
        .args(["--reuid", &OTHER_UID.to_string(), "--regid", &OTHER_UID.to_string(), "--clear-groups", "sh", "-c", script, "sh"])
        .arg(arg)
        .output()
        .expect("running setpriv (util-linux)")
}

#[test]
#[ignore]
fn socket_fallback_is_private_to_its_user() {
    let _serial = serial();
    let root = temp_root();
    let _daemon = spawn_daemon(&root, None);
    let socket = socket_in(&root);
    wait_for("the daemon's socket", || UnixStream::connect(&socket).ok());

    let dir = socket.parent().unwrap();
    let dir_meta = fs::metadata(dir).unwrap();
    assert_eq!(dir_meta.permissions().mode() & 0o777, 0o700, "the directory is private");
    assert_eq!(dir_meta.uid(), own_uid(), "the directory belongs to the daemon's user");
    assert_eq!(fs::metadata(&socket).unwrap().permissions().mode() & 0o777, 0o600, "the socket is private");

    // Positive control: setpriv works, so a refusal below means something.
    let control = as_other_user("exit 0", &root);
    assert!(control.status.success(), "setpriv runs the other user: {}", String::from_utf8_lossy(&control.stderr));

    let refused = as_other_user("exec 3<>\"$1\"", &socket);
    assert!(!refused.status.success(), "the other user must not open the daemon's socket");
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("Permission denied"), "refused for permissions, not something else: {stderr}");

    let _ = fs::remove_dir_all(&root);
}

#[test]
#[ignore]
fn socket_fallback_refuses_a_directory_it_does_not_own() {
    let _serial = serial();
    let root = temp_root();
    let dir = root.join("tmp").join(format!("padwight-{}", own_uid()));
    fs::create_dir_all(&dir).unwrap();
    chown(&dir, Some(OTHER_UID), Some(OTHER_UID)).unwrap();

    let mut daemon = spawn_daemon(&root, None);
    let status = wait_for("the daemon to give up", || daemon.0.try_wait().unwrap());
    assert!(!status.success(), "the daemon must not start on another user's directory");
    let log = daemon_log(&root);
    assert!(log.contains("not a directory owned by this user"), "the reason is logged: {log}");
    assert!(!dir.join("padwight.sock").exists(), "no socket was created there");

    let _ = fs::remove_dir_all(&root);
}
