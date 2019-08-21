use std::process::Command;
use std::thread;
use std::time::Duration;

use dbus;

use errors::*;

const SYSTEMD: &str = "org.freedesktop.systemd1";
const SYSTEMD_MANAGER: &str = "org.freedesktop.systemd1.Manager";
const SYSTEMD_PATH: &str = "/org/freedesktop/systemd1";

const DEFAULT_MODE: &str = "replace";

pub fn start_service(name: &str) -> Result<()> {
    start_service_impl(name).chain_err(|| ErrorKind::StartService(name.into()))
}

fn start_service_impl(name: &str) -> Result<()> {
    info!("Starting {}...", name);

    let connection = dbus::Connection::get_private(dbus::BusType::System)?;

    let path = connection.with_path(SYSTEMD, SYSTEMD_PATH, 5000);

    path.start_unit(name, DEFAULT_MODE)?;

    Ok(())
}

pub fn stop_service(name: &str) -> Result<()> {
    stop_service_impl(name).chain_err(|| ErrorKind::StopService(name.into()))
}

fn stop_service_impl(name: &str) -> Result<()> {
    info!("Stopping {}...", name);

    let connection = dbus::Connection::get_private(dbus::BusType::System)?;

    let path = connection.with_path(SYSTEMD, SYSTEMD_PATH, 5000);

    path.stop_unit(name, DEFAULT_MODE)?;

    Ok(())
}

pub fn reload_or_restart_service(name: &str) -> Result<()> {
    reload_or_restart_service_impl(name).chain_err(|| ErrorKind::ReloadRestartService(name.into()))
}

fn reload_or_restart_service_impl(name: &str) -> Result<()> {
    info!("Reloading or restarting {}...", name);

    let connection = dbus::Connection::get_private(dbus::BusType::System)?;

    let path = connection.with_path(SYSTEMD, SYSTEMD_PATH, 5000);

    path.reload_or_restart_unit(name, DEFAULT_MODE)?;

    Ok(())
}

pub fn await_service_exit(name: &str) -> Result<()> {
    await_service_exit_impl(name).chain_err(|| ErrorKind::AwaitServiceExit(name.into()))
}

pub fn await_service_exit_impl(name: &str) -> Result<()> {
    info!("Awaiting {} to exit...", name);

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

    bail!(ErrorKind::AwaitServiceExitTimeout)
}

pub fn service_exists(name: &str) -> bool {
    match service_exists_impl(name) {
        Ok(result) => result,
        Err(_) => false,
    }
}

pub fn restart_service_later(name: &str, delay_sec: u64) -> std::result::Result<i32, &'static str> {
    let status = Command::new("/usr/bin/systemd-run")
        .arg(format!("--on-active={}", delay_sec))
        .arg("--timer-property=AccuracySec=100ms")
        .arg("systemctl")
        .arg("restart")
        .arg(name)
        .status()
        .expect("Error calling systemd-run");
    
    match status.code() {
        Some(code) => Ok(code),
        None => Err("systemd-run was terminated by signal")
    }
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
