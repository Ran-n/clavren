// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/14 18:09:54.649645
//+ Revised: 	2026/11/14 18:09:54.649645
// -----------------------------------------------------------------------------

//! What the fake server says about the application it took, as the test decided it.

use super::*;

impl FakeServer {
    pub(super) fn application_status(&self, now: DateTime<Utc>) -> Answer {
        let decision = self.decision.lock().unwrap().clone();
        let (status, key, reason) = match decision {
            Decision::Unknown => {
                return answer(404, json!({ "error": "application_unknown", "message": "no", "server_time": now }));
            }
            Decision::Pending => ("pending", None, None),
            Decision::Approved => ("approved", Some(APPROVED_KEY), None),
            Decision::Rejected(reason) => ("rejected", None, reason),
            Decision::Withdrawn => ("withdrawn", None, None),
        };
        answer(
            200,
            json!({
                "application": APPLICATION,
                "product": PRODUCT,
                "status": status,
                "created_at": now,
                "decided_at": if status == "pending" { Value::Null } else { json!(now) },
                "reason": reason,
                "key": key,
                "licence_expires_at": null,
                "server_time": now,
            }),
        )
    }
}
