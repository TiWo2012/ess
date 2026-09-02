//! Probe the OS keyring: `keyring_probe [service] [user] [password]`
//! - no args: set/get/delete a self-test entry
//! - service+user: print the stored password (for the integration test)
//! - service+user+password: set it

use anyhow::{anyhow, Result};
use keyring::Entry;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => {
            let e = Entry::new("ess-probe", "self-test")?;
            e.set_password("probe-value")?;
            assert_eq!(e.get_password()?, "probe-value");
            e.delete_credential()?;
            println!(
                "set/get/delete ok; store_status: {:?}",
                Entry::store_status().is_ok()
            );
        }
        [service, user] => {
            let e = Entry::new(service, user)?;
            match e.get_password() {
                Ok(pw) => println!("{pw}"),
                Err(err) => return Err(anyhow!("GET failed: {err:?}")),
            }
        }
        [service, user, password] => {
            let e = Entry::new(service, user)?;
            e.set_password(password)?;
            println!("SET ok");
        }
        _ => return Err(anyhow!("usage: keyring_probe [service [user [password]]]")),
    }
    Ok(())
}
