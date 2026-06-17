//! The decision tick and its parts. No serde derives: canonical and on-disk
//! encodings are built by hand (tick.rs / canonical.rs) for exact byte control.

#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    pub id: String,          // bookkeeping (the hash output)
    pub parent_id: String,   // hashed; "" on genesis, present
    pub observe: String,     // hashed
    pub decision: String,    // hashed
    pub grounds: Vec<Ground>,// hashed
    pub status: String,      // bookkeeping
    pub held_since: String,  // bookkeeping
    pub blame: String,       // bookkeeping
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ground {
    pub claim: String,
    pub supports: String,        // "chosen" | "rejected:<option>"
    pub check: Option<Check>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Check {
    Person { reference: String },                 // by=person, ref=note
    Test {
        reference: String,                        // by=test, ref=selector
        verified_at_sha: String,                  // 40 lowercase hex
        counter_test: String,
        liveness: Liveness,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Liveness {
    pub platforms: Vec<String>,
    pub triggered_by: Vec<String>,
    pub surfaces: Vec<String>,
}
