use std::env;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use zbus::blocking::Connection;
use zbus::dbus_proxy;
use zbus::zvariant::OwnedObjectPath;

const DEFAULT_MODE: &str = "replace";

const MOCK_SYSTEMD: &str = "MOCK_SYSTEMD";

pub fn start_service(name: &str) -> Result<()> {
    info!("Starting {name}...");

    if should_mock_systemd() {
        return Ok(());
    }
    start_service_impl(name).context(format!("Starting {name} failed"))
}

fn start_service_impl(name: &str) -> Result<()> {
    let connection = Connection::system()?;

    let manager = ManagerProxyBlocking::new(&connection)?;

    manager.start_unit(name, DEFAULT_MODE)?;

    Ok(())
}

pub fn stop_service(name: &str) -> Result<()> {
    info!("Stopping {name}...");

    if should_mock_systemd() {
        return Ok(());
    }
    stop_service_impl(name).context(format!("Stopping {name} failed"))
}

fn stop_service_impl(name: &str) -> Result<()> {
    let connection = Connection::system()?;

    let manager = ManagerProxyBlocking::new(&connection)?;

    manager.stop_unit(name, DEFAULT_MODE)?;

    Ok(())
}

pub fn reload_or_restart_service(name: &str) -> Result<()> {
    info!("Reloading or restarting {name}...");

    if should_mock_systemd() {
        return Ok(());
    }
    reload_or_restart_service_impl(name).context(format!("Reloading or restarting {name} failed"))
}

fn reload_or_restart_service_impl(name: &str) -> Result<()> {
    let connection = Connection::system()?;

    let manager = ManagerProxyBlocking::new(&connection)?;

    manager.reload_or_restart_unit(name, DEFAULT_MODE)?;

    Ok(())
}

pub fn await_service_exit(name: &str) -> Result<()> {
    info!("Awaiting {name} to exit...");

    if should_mock_systemd() {
        return Ok(());
    }
    await_service_exit_impl(name).context(format!("Awaiting {name} to exit failed"))
}

fn await_service_exit_impl(name: &str) -> Result<()> {
    let connection = Connection::system()?;

    let manager = ManagerProxyBlocking::new(&connection)?;

    let unit_path = manager.get_unit(name)?;
    let unit = UnitProxyBlocking::builder(&connection)
        .path(&unit_path)?
        .build()?;

    for _ in 0..90 {
        let active_state = unit.active_state()?;

        if active_state == "inactive" || active_state == "failed" {
            return Ok(());
        }

        thread::sleep(Duration::from_secs(1));
    }

    bail!("Timed out awaiting service to exit")
}

pub fn service_exists(name: &str) -> bool {
    if should_mock_systemd() {
        return true;
    }
    service_exists_impl(name).unwrap_or(false)
}

fn service_exists_impl(name: &str) -> Result<bool> {
    let connection = Connection::system()?;

    let manager = ManagerProxyBlocking::new(&connection)?;

    match manager.get_unit(name) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

fn should_mock_systemd() -> bool {
    if let Ok(mock) = env::var(MOCK_SYSTEMD) {
        mock == "1"
    } else {
        false
    }
}

#[dbus_proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
trait Manager {
    fn get_unit(&self, name: &str) -> zbus::Result<OwnedObjectPath>;
    fn start_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    fn stop_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    fn reload_or_restart_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
}

#[dbus_proxy(
    interface = "org.freedesktop.systemd1.Unit",
    default_service = "org.freedesktop.systemd1"
)]
trait Unit {
    #[dbus_proxy(property)]
    fn active_state(&self) -> zbus::Result<String>;
}
