//! Mii Support

use crate::result::*;
use crate::sync::{Mutex, MutexGuard};

pub use crate::service::mii::*;

static G_STATIC_SRV: Mutex<Option<StaticService>> = Mutex::new(None);
static G_DB_SRV: Mutex<Option<MiiDatabase>> = Mutex::new(None);

/// Initializes the Mii service objects
pub fn initialize() -> Result<()> {
    let static_service = crate::service::new_service_object::<StaticService>()?;
    let db_service = static_service.get_database_service(SpecialKeyCode::Normal)?;
    *G_STATIC_SRV.lock() = Some(static_service);
    *G_DB_SRV.lock() = Some(db_service);
    Ok(())
}

/// Gets access to the global [`IStaticClient`] shared object instance
pub fn get_static_service<'a>() -> MutexGuard<'a, Option<StaticService>> {
    G_STATIC_SRV.lock()
}

/// Gets access to the global [`IMiiDatabaseClient`] shared object instance
pub fn get_mii_database<'a>() -> MutexGuard<'a, Option<MiiDatabase>> {
    G_DB_SRV.lock()
}

pub(crate) fn finalize() {
    *G_DB_SRV.lock() = None;
    *G_STATIC_SRV.lock() = None;
}

/// Gets the Mii author ID for the current user.
#[inline]
pub fn get_device_id() -> Result<CreateId> {
    use crate::service::set::{ISystemSettingsClient, SystemSettingsService};

    crate::service::new_service_object::<SystemSettingsService>()?.get_mii_author_id()
}
