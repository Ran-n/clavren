// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/11 16:32:48.289637
//+ Revised: 	2026/11/11 17:53:42.650663
// -----------------------------------------------------------------------------

//! Applying for a licence from inside the application: send the applicant's name, address and an
//! optional message; keep the secret the server answers with; ask now and then how it went; and,
//! once approved, activate the key the answer carries, so the applicant never has to type it. A
//! rejection is kept, with the administrator's reason, until the application is set aside.

use clavren_protocol::wire::{ApplicationCreated, ApplicationStatus, ApplicationStatusResponse, ErrorCode};
use serde_json::json;
use uuid::Uuid;
use zeroize::Zeroize;

use crate::client::Client;
use crate::online::{error_code, parsed, retry_after};
use crate::outcome::{Applied, Outcome, Temporary};
use crate::state::Application;
use crate::store::StoreError;

/// Who applies for the licence, as they typed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applicant {
    pub name: String,
    pub email: String,
    /// Anything the applicant wants the administrator to know (at most 2,000 characters).
    pub message: Option<String>,
}

impl Client {
    /// Sends a licence application for this product. Once sent, [`Self::status`] says
    /// [`Outcome::ApplicationPending`], and [`Self::check`] (so the background checker) asks how
    /// it went every 15 minutes. Sending one sets aside the licence this computer had, if it was
    /// locked (revoked, expired or withdrawn). `Err` when the server took the application but its
    /// secret couldn't be saved: sending again then answers [`Applied::AlreadyPending`] until the
    /// administrator decides, since only the lost secret could ask how it went.
    pub fn apply(&self, applicant: &Applicant) -> Result<Applied, StoreError> {
        if self.status().is_licensed() {
            return Ok(Applied::AlreadyLicensed);
        }
        let body = json!({
            "product": self.config.product,
            "name": applicant.name,
            "email": applicant.email,
            "message": applicant.message,
            "source": "app",
        });
        let answer = match self.transport.post("/api/v1/applications", &body) {
            Ok(answer) => answer,
            Err(error) => {
                log::warn!("the licence application couldn't reach the licence server: {error}");
                return Ok(Applied::Unreachable);
            }
        };
        if answer.status != 201 {
            return Ok(match error_code(&answer) {
                Some(ErrorCode::ApplicationPending) => Applied::AlreadyPending,
                Some(ErrorCode::InvalidRequest) => Applied::InvalidDetails,
                Some(ErrorCode::ProductUnavailable) => Applied::ProductUnavailable,
                Some(ErrorCode::RateLimited) => Applied::RateLimited { retry_after: retry_after(&answer) },
                other => {
                    log::error!("the licence application failed: HTTP {} ({other:?})", answer.status);
                    Applied::Unreachable
                }
            });
        }
        let Some(ApplicationCreated { application, secret, .. }) = parsed(&answer) else {
            log::error!("the licence application got HTTP 201 without an application and a secret");
            return Ok(Applied::Unreachable);
        };
        let mut state = self.lock();
        let mut sent = state.clone();
        sent.application = Some(Application::Pending { id: application, secret, last_check: None });
        // a locked licence is over: the application replaces it
        sent.refusal = None;
        sent.activation = None;
        sent.token = None;
        self.save(&sent)?;
        *state = sent;
        log::info!("licence application sent");
        Ok(Applied::Sent)
    }

    /// Asks how the waiting licence application went. Approved: activates its key and returns the
    /// outcome of activating (usually [`Outcome::Licensed`]). Rejected or withdrawn: keeps the
    /// rejection and returns [`Outcome::ApplicationRejected`]. Still waiting:
    /// [`Outcome::ApplicationPending`]. Without an application waiting, the current status.
    /// Anything temporary (no network, a server error, a state that can't be saved) is `Err` and
    /// changes nothing.
    pub fn check_application(&self) -> Result<Outcome, Temporary> {
        let Some((id, secret)) = self.pending() else { return Ok(self.status()) };
        let answer = self.transport.post("/api/v1/applications/status", &json!({ "secret": secret }));
        let answer = answer.map_err(Temporary::Unreachable)?;
        if answer.status != 200 {
            return match error_code(&answer) {
                // deleted by the administrator: it won't be decided now
                Some(ErrorCode::ApplicationUnknown) => self.rejected(id, None),
                Some(ErrorCode::RateLimited) => Err(Temporary::RateLimited(retry_after(&answer))),
                other => Err(Temporary::Server(format!("HTTP {} ({other:?})", answer.status))),
            };
        }
        let Some(mut status) = parsed::<ApplicationStatusResponse>(&answer).filter(|status| status.application == id)
        else {
            return Err(Temporary::Server("the application's status is malformed".into()));
        };
        match (status.status, status.key.take()) {
            (ApplicationStatus::Approved, Some(mut key)) => {
                let activated = self.activate_approved(id, &key);
                key.zeroize();
                activated
            }
            (ApplicationStatus::Pending, _) => self.still_pending(id),
            (ApplicationStatus::Rejected, _) => self.rejected(id, status.reason.take()),
            (ApplicationStatus::Withdrawn, _) => self.rejected(id, None),
            (ApplicationStatus::Approved, None) => Err(Temporary::Server("an approval without a key".into())),
        }
    }

    /// Sets aside the licence application (waiting or rejected): [`Self::status`] goes back to
    /// [`Outcome::NeedsActivation`]. A waiting one stays waiting on the server, where the
    /// administrator can still decide it; this computer just stops asking.
    pub fn dismiss_application(&self) -> Result<(), StoreError> {
        let mut state = self.lock();
        if state.application.is_none() {
            return Ok(());
        }
        let mut dismissed = state.clone();
        dismissed.application = None;
        self.save(&dismissed)?;
        *state = dismissed;
        Ok(())
    }

    /// The waiting application's ID and secret.
    fn pending(&self) -> Option<(Uuid, String)> {
        match &self.lock().application {
            Some(Application::Pending { id, secret, .. }) => Some((*id, secret.clone())),
            _ => None,
        }
    }

    fn activate_approved(&self, id: Uuid, key: &str) -> Result<Outcome, Temporary> {
        // the application may have been set aside, or a key typed, while the answer came
        if self.pending().map(|(pending, _)| pending) != Some(id) {
            return Err(Temporary::Server("the application changed while asking about it".into()));
        }
        let outcome = self.activate(key)?;
        match outcome {
            // activating ended the application; the outcome kinds below would come again
            Outcome::Licensed { .. } => Ok(outcome),
            Outcome::Unreachable => Err(Temporary::Unreachable("activating the approved key".into())),
            Outcome::RateLimited { retry_after } => Err(Temporary::RateLimited(retry_after)),
            refused => {
                log::warn!("the approved licence couldn't be activated: {refused:?}");
                self.dismiss_application()?;
                Ok(refused)
            }
        }
    }

    fn still_pending(&self, id: Uuid) -> Result<Outcome, Temporary> {
        let now = self.clock.now();
        self.update_application(id, |application| {
            if let Application::Pending { last_check, .. } = application {
                *last_check = Some(now);
            }
        })?;
        Ok(Outcome::ApplicationPending)
    }

    fn rejected(&self, id: Uuid, reason: Option<String>) -> Result<Outcome, Temporary> {
        let kept = reason.clone();
        self.update_application(id, move |application| *application = Application::Rejected { reason: kept })?;
        log::warn!("the licence application was rejected");
        Ok(Outcome::ApplicationRejected { reason })
    }

    /// Changes the waiting application `id` and saves it; `Err` when another took its place.
    fn update_application(&self, id: Uuid, change: impl FnOnce(&mut Application)) -> Result<(), Temporary> {
        let mut state = self.lock();
        let mut updated = state.clone();
        let current = matches!(&updated.application, Some(Application::Pending { id: pending, .. }) if *pending == id);
        match updated.application.as_mut() {
            Some(application) if current => change(application),
            _ => return Err(Temporary::Server("the application changed while asking about it".into())),
        }
        self.save(&updated)?;
        *state = updated;
        Ok(())
    }
}
