use dbus;

use errors::*;

const SYSTEMD: &str = "org.freedesktop.systemd1";
const SYSTEMD_MANAGER: &str = "org.freedesktop.systemd1.Manager";
const SYSTEMD_PATH: &str = "/org/freedesktop/systemd1";

pub fn reload_or_restart_service(name: &str) -> Result<()> {
    reload_or_restart_service_impl(name).chain_err(|| ErrorKind::ReloadRestartService(name.into()))
}

fn reload_or_restart_service_impl(name: &str) -> Result<()> {
    println!("Reloading or restarting {}...", name);

    let connection = dbus::Connection::get_private(dbus::BusType::System)?;

    let path = connection.with_path(SYSTEMD, SYSTEMD_PATH, 5000);

    path.reload_or_restart_unit(name, "replace")?;

    Ok(())
}

pub trait OrgFreedesktopSystemd1Manager {
    fn reload_or_restart_unit(&self, name: &str, mode: &str) -> Result<dbus::Path<'static>>;
}

impl<'a, C: ::std::ops::Deref<Target = dbus::Connection>> OrgFreedesktopSystemd1Manager
    for dbus::ConnPath<'a, C>
{
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
