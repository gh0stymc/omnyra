//! Security module (step 6c): account audit checklist, phishing detection in comments,
//! impersonation suspects, app lock (PIN) and the audit log viewer.
//!
//! Decision D7: the PIN lock protects the **interface** only; the database is not
//! encrypted (SQLCipher was not added). The UI says so.

pub mod checklist;
pub mod impersonation;
pub mod lock;
pub mod phishing;
