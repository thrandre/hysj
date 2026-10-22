//! The Hysj daemon as its own program (for a login item later). Clients
//! normally start the daemon by running themselves with `--daemon`.

fn main() {
    hysjd::daemon_main();
}
