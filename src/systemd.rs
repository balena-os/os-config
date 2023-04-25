use std::env;
use std::thread;
use std::time::Duration;

use dbus;

use anyhow::{bail, Context, Result};

const SYSTEMD: &str = "org.freedesktop.systemd1";
const SYSTEMD_MANAGER: &str = "org.freedesktop.systemd1.Manager";
const SYSTEMD_PATH: &str = "/org/freedesktop/systemd1";

const DEFAULT_MODE: &str = "replace";

const MOCK_SYSTEMD: &str = "MOCK_SYSTEMD";

pub fn start_service(name: &str) -> Result<()> {
    info!("Starting {}...", name);

    if should_mock_systemd() {
        return Ok(());
    }
    start_service_impl(name).context(format!("Starting {} failed", name))
}

fn start_service_impl(name: &str) -> Result<()> {
    let connection = dbus::Connection::get_private(dbus::BusType::System)?;

    let path = connection.with_path(SYSTEMD, SYSTEMD_PATH, 5000);

    path.start_unit(name, DEFAULT_MODE)?;

    Ok(())
}

pub fn stop_service(name: &str) -> Result<()> {
    info!("Stopping {}...", name);

    if should_mock_systemd() {
        return Ok(());
    }
    stop_service_impl(name).context(format!("Stopping {} failed", name))
}

fn stop_service_impl(name: &str) -> Result<()> {
    let connection = dbus::Connection::get_private(dbus::BusType::System)?;

    let path = connection.with_path(SYSTEMD, SYSTEMD_PATH, 5000);

    path.stop_unit(name, DEFAULT_MODE)?;

    Ok(())
}

pub fn reload_or_restart_service(name: &str) -> Result<()> {
    info!("Reloading or restarting {}...", name);

    if should_mock_systemd() {
        return Ok(());
    }
    reload_or_restart_service_impl(name).context(format!("Reloading or restarting {} failed", name))
}

fn reload_or_restart_service_impl(name: &str) -> Result<()> {
    let connection = dbus::Connection::get_private(dbus::BusType::System)?;

    let path = connection.with_path(SYSTEMD, SYSTEMD_PATH, 5000);

    path.reload_or_restart_unit(name, DEFAULT_MODE)?;

    Ok(())
}

pub fn await_service_exit(name: &str) -> Result<()> {
    info!("Awaiting {} to exit...", name);

    if should_mock_systemd() {
        return Ok(());
    }
    await_service_exit_impl(name).context(format!("Awaiting {} to exit failed", name))
}

fn await_service_exit_impl(name: &str) -> Result<()> {
    let connection = dbus::Connection::get_private(dbus::BusType::System)?;

    let path = connection.with_path(SYSTEMD, SYSTEMD_PATH, 5000);

    let unit_path = connection.with_path(SYSTEMD, path.get_unit(name)?, 5000);

    for _ in 0..90 {
        let active_state = unit_path.get_active_state()?;

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
    let connection = dbus::Connection::get_private(dbus::BusType::System)?;

    let path = connection.with_path(SYSTEMD, SYSTEMD_PATH, 5000);

    match path.get_unit(name) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

pub trait OrgFreedesktopSystemd1Manager {
    fn get_unit(&self, name: &str) -> Result<dbus::Path<'static>>;
    fn start_unit(&self, name: &str, mode: &str) -> Result<dbus::Path<'static>>;
    fn stop_unit(&self, name: &str, mode: &str) -> Result<dbus::Path<'static>>;
    fn reload_or_restart_unit(&self, name: &str, mode: &str) -> Result<dbus::Path<'static>>;
}

pub trait OrgFreedesktopSystemd1Unit {
    fn get_active_state(&self) -> Result<String>;
}

impl<'a, C: ::std::ops::Deref<Target = dbus::Connection>> OrgFreedesktopSystemd1Manager
    for dbus::ConnPath<'a, C>
{
    fn get_unit(&self, arg0: &str) -> Result<dbus::Path<'static>> {
        let mut method =
            self.method_call_with_args(&SYSTEMD_MANAGER.into(), &"GetUnit".into(), |msg| {
                let mut i = dbus::arg::IterAppend::new(msg);
                i.append(arg0);
            })?;
        method.as_result()?;
        let mut iter = method.iter_init();
        let unit: dbus::Path<'static> = iter.read()?;
        Ok(unit)
    }

    fn start_unit(&self, name: &str, mode: &str) -> Result<dbus::Path<'static>> {
        let mut method =
            self.method_call_with_args(&SYSTEMD_MANAGER.into(), &"StartUnit".into(), |msg| {
                let mut i = dbus::arg::IterAppend::new(msg);
                i.append(name);
                i.append(mode);
            })?;
        method.as_result()?;
        let mut iter = method.iter_init();
        let job: dbus::Path<'static> = iter.read()?;
        Ok(job)
    }

    fn stop_unit(&self, name: &str, mode: &str) -> Result<dbus::Path<'static>> {
        let mut method =
            self.method_call_with_args(&SYSTEMD_MANAGER.into(), &"StopUnit".into(), |msg| {
                let mut i = dbus::arg::IterAppend::new(msg);
                i.append(name);
                i.append(mode);
            })?;
        method.as_result()?;
        let mut iter = method.iter_init();
        let job: dbus::Path<'static> = iter.read()?;
        Ok(job)
    }

    fn reload_or_restart_unit(&self, name: &str, mode: &str) -> Result<dbus::Path<'static>> {
        let mut method = self.method_call_with_args(
            &SYSTEMD_MANAGER.into(),
            &"ReloadOrRestartUnit".into(),
            |msg| {
                let mut i = dbus::arg::IterAppend::new(msg);
                i.append(name);
                i.append(mode);
            },
        )?;
        method.as_result()?;
        let mut iter = method.iter_init();
        let job: dbus::Path<'static> = iter.read()?;
        Ok(job)
    }
}

impl<'a, C: ::std::ops::Deref<Target = dbus::Connection>> OrgFreedesktopSystemd1Unit
    for dbus::ConnPath<'a, C>
{
    fn get_active_state(&self) -> Result<String> {
        let active_state = <Self as dbus::stdintf::org_freedesktop_dbus::Properties>::get(
            self,
            "org.freedesktop.systemd1.Unit",
            "ActiveState",
        )?;
        Ok(active_state)
    }
}

fn should_mock_systemd() -> bool {
    if let Ok(mock) = env::var(MOCK_SYSTEMD) {
        mock == "1"
    } else {
        false
    }
}
