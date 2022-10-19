//! Operations service's consumer metrics.

use lazy_static::lazy_static;
use prometheus::IntGauge;

lazy_static! {
    pub static ref HEIGHT: IntGauge = IntGauge::new("Height", "Currently imported height")
        .expect("can't create Height metric");
    pub static ref UPDATES_BATCH_SIZE: IntGauge = IntGauge::new("UpdatesBatchSize", "Number of updates in each batch")
        .expect("can't create UpdatesBatchSize metric");
    pub static ref UPDATES_BATCH_TIME: IntGauge = IntGauge::new("UpdatesBatchTimeMs", "Time (in ms) of each batch")
        .expect("can't create UpdatesBatchTimeMs metric");
    pub static ref DB_WRITE_TIME: IntGauge = IntGauge::new("DatabaseWriteTimeMs", "Time (in ms) of DB writes")
        .expect("can't create DatabaseWriteTimeMs metric");
}
