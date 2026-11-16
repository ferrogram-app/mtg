// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::fmt;

use mtg_crypto::two_factor_auth::{calculate_2fa, check_p_and_g};
use mtg_mtsender::InvocationError;
use mtg_session::types::{PeerInfo, UpdateState, UpdatesState};
use mtg_tl_types as tl;

use super::Client;
use crate::peer::User;
use crate::utils;

mod password;
mod types;

pub use types::{LoginToken, PasswordToken, SignInError};

/// Method implementations related with the authentication of the user into the API.
///
/// Most requests to the API require the user to have authorized their key, stored in the session,
/// before being able to use them.
impl Client {
    /// Returns `true` if the current account is authorized. Otherwise,
    /// logging in will be required before being able to invoke requests.
    ///
    /// This will likely be the first method you want to call on a connected [`Client`]. After you
    /// determine if the account is authorized or not, you will likely want to use either
    /// [`Client::bot_sign_in`] or [`Client::request_login_code`].
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// if client.is_authorized().await? {
    ///     println!("Client already authorized and ready to use!");
    /// } else {
    ///     println!("Client is not authorized, you will need to sign_in!");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn is_authorized(&self) -> Result<bool, InvocationError> {
        match self.invoke(&tl::functions::updates::GetState {}).await {
            Ok(_) => Ok(true),
            Err(InvocationError::Rpc(e)) if e.code == 401 => Ok(false),
            Err(err) => Err(err),
        }
    }

    async fn complete_login(
        &self,
        auth: tl::types::auth::Authorization,
    ) -> Result<User, InvocationError> {
        // In the extremely rare case where `Err` happens, there's not much we can do.
        // `message_box` will try to correct its state as updates arrive.
        let update_state = self.invoke(&tl::functions::updates::GetState {}).await.ok();

        let user = User::from_raw(self, auth.user);
        let auth = user.to_ref().await?.unwrap().auth;

        self.0
            .session
            .cache_peer(PeerInfo::User {
                id: user.id().bare_id_unchecked(),
                auth: Some(auth),
                bot: Some(user.is_bot()),
                is_self: Some(true),
            })
            .await?;
        if let Some(tl::enums::updates::State::State(state)) = update_state {
            self.0
                .session
                .set_update_state(UpdateState::All(UpdatesState {
                    pts: state.pts,
                    qts: state.qts,
                    date: state.date,
                    seq: state.seq,
                    channels: Vec::new(),
                }))
                .await?;
        }

        Ok(user)
    }

    /// Signs in to the bot account associated with this token.
    ///
    /// This is the method you need to call to use the client under a bot account.
    ///
    /// It is recommended to save the session on successful login. Some session storages will do this
    /// automatically. If saving fails, it is recommended to [`Client::sign_out`]. If the session is never
    /// saved post-login, then the authorization will be "lost" in the list of logged-in clients, since it
    /// is unaccessible.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// // Note: these values are obviously fake.
    /// //       Obtain your own with the developer's phone at https://my.telegram.org.
    /// const API_HASH: &str = "514727c32270b9eb8cc16daf17e21e57";
    /// //       Obtain your own by talking to @BotFather via a Telegram app.
    /// const TOKEN: &str = "776609994:AAFXAy5-PawQlnYywUlZ_b_GOXgarR3ah_yq";
    ///
    /// let user = match client.bot_sign_in(TOKEN, API_HASH).await {
    ///     Ok(user) => user,
    ///     Err(err) => {
    ///         println!("Failed to sign in as a bot :(\n{}", err);
    ///         return Err(err.into());
    ///     }
    /// };
    ///
    /// if let Some(first_name) = user.first_name() {
    ///     println!("Signed in as {}!", first_name);
    /// } else {
    ///     println!("Signed in!");
    /// }
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub async fn bot_sign_in(&self, token: &str, api_hash: &str) -> Result<User, InvocationError> {
        let request = tl::functions::auth::ImportBotAuthorization {
            flags: 0,
            api_id: self.0.api_id,
            api_hash: api_hash.to_string(),
            bot_auth_token: token.to_string(),
        };

        let result = match self.invoke(&request).await {
            Ok(x) => x,
            Err(InvocationError::Rpc(err)) if err.code == 303 => {
                let old_dc_id = self.0.session.home_dc_id()?;
                let Some(new_dc_id) = err.value else {
                    return Err(InvocationError::Rpc(err));
                };
                let new_dc_id = new_dc_id as i32;
                // Disconnect from current DC to cull the now-unused connection.
                // This also gives a chance for the new home DC to export its authorization
                // if there's a need to connect back to the old DC after having logged in.
                self.0.handle.disconnect_from_dc(old_dc_id);
                self.0.session.set_home_dc_id(new_dc_id).await?;
                self.invoke(&request).await?
            }
            Err(e) => return Err(e),
        };

        match result {
            tl::enums::auth::Authorization::Authorization(x) => self.complete_login(x).await,
            tl::enums::auth::Authorization::SignUpRequired(_) => {
                Err(InvocationError::Dropped("SignUpRequired for a bot login"))
            }
        }
    }

    /// Requests the login code for the account associated to the given phone
    /// number via another Telegram application or SMS.
    ///
    /// This is the method you need to call before being able to sign in to a user account.
    /// After you obtain the code and it's inside your program (e.g. ask the user to enter it
    /// via the console's standard input), you will need to [`Client::sign_in`] to complete the
    /// process.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// // Note: these values are obviously fake.
    /// //       Obtain your own with the developer's phone at https://my.telegram.org.
    /// const API_HASH: &str = "514727c32270b9eb8cc16daf17e21e57";
    /// //       The phone used here does NOT need to be the same as the one used by the developer
    /// //       to obtain the API ID and hash.
    /// const PHONE: &str = "+1 415 555 0132";
    ///
    /// if !client.is_authorized().await? {
    ///     // We're not logged in, so request the login code.
    ///     client.request_login_code(PHONE, API_HASH).await?;
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn request_login_code(
        &self,
        phone: &str,
        api_hash: &str,
    ) -> Result<LoginToken, InvocationError> {
        let request = tl::functions::auth::SendCode {
            phone_number: phone.to_string(),
            api_id: self.0.api_id,
            api_hash: api_hash.to_string(),
            settings: tl::types::CodeSettings {
                allow_flashcall: false,
                current_number: false,
                allow_app_hash: false,
                allow_missed_call: false,
                allow_firebase: false,
                logout_tokens: None,
                token: None,
                app_sandbox: None,
                unknown_number: false,
            }
            .into(),
        };

        use tl::enums::auth::SentCode as SC;

        let sent_code: tl::types::auth::SentCode = match self.invoke(&request).await {
            Ok(x) => match x {
                SC::Code(code) => code,
                SC::Success(_) => {
                    return Err(InvocationError::Dropped("unexpected SentCodeSuccess"));
                }
                SC::PaymentRequired(_) => {
                    return Err(InvocationError::Dropped(
                        "SentCodePaymentRequired is unsupported",
                    ));
                }
            },
            Err(InvocationError::Rpc(err)) if err.code == 303 => {
                let old_dc_id = self.0.session.home_dc_id()?;
                let Some(new_dc_id) = err.value else {
                    return Err(InvocationError::Rpc(err));
                };
                let new_dc_id = new_dc_id as i32;
                // Disconnect from current DC to cull the now-unused connection.
                // This also gives a chance for the new home DC to export its authorization
                // if there's a need to connect back to the old DC after having logged in.
                self.0.handle.disconnect_from_dc(old_dc_id);
                self.0.session.set_home_dc_id(new_dc_id).await?;
                match self.invoke(&request).await? {
                    SC::Code(code) => code,
                    SC::Success(_) => {
                        return Err(InvocationError::Dropped("unexpected SentCodeSuccess"));
                    }
                    SC::PaymentRequired(_) => {
                        return Err(InvocationError::Dropped(
                            "SentCodePaymentRequired is unsupported",
                        ));
                    }
                }
            }
            Err(e) => return Err(e),
        };

        Ok(LoginToken {
            phone: phone.to_string(),
            phone_code_hash: sent_code.phone_code_hash,
        })
    }

    /// Signs in to the user account.
    ///
    /// You must call [`Client::request_login_code`] before using this method in order to obtain
    /// necessary login token, and also have asked the user for the login code.
    ///
    /// It is recommended to save the session on successful login. Some session storages will do this
    /// automatically. If saving fails, it is recommended to [`Client::sign_out`]. If the session is never
    /// saved post-login, then the authorization will be "lost" in the list of logged-in clients, since it
    /// is unaccessible.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mtg_client::SignInError;
    ///
    ///  async fn f(client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// # const API_HASH: &str = "";
    /// # const PHONE: &str = "";
    /// fn ask_code_to_user() -> String {
    ///     unimplemented!()
    /// }
    ///
    /// let token = client.request_login_code(PHONE, API_HASH).await?;
    /// let code = ask_code_to_user();
    ///
    /// let user = match client.sign_in(&token, &code).await {
    ///     Ok(user) => user,
    ///     Err(SignInError::PasswordRequired(_token)) => panic!("Please provide a password"),
    ///     Err(SignInError::SignUpRequired) => panic!("Sign up required"),
    ///     Err(err) => {
    ///         println!("Failed to sign in as a user :(\n{}", err);
    ///         return Err(err.into());
    ///     }
    /// };
    ///
    /// if let Some(first_name) = user.first_name() {
    ///     println!("Signed in as {}!", first_name);
    /// } else {
    ///   println!("Signed in!");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn sign_in(&self, token: &LoginToken, code: &str) -> Result<User, SignInError> {
        match self
            .invoke(&tl::functions::auth::SignIn {
                phone_number: token.phone.clone(),
                phone_code_hash: token.phone_code_hash.clone(),
                phone_code: Some(code.to_string()),
                email_verification: None,
            })
            .await
        {
            Ok(tl::enums::auth::Authorization::Authorization(x)) => {
                self.complete_login(x).await.map_err(SignInError::Other)
            }
            Ok(tl::enums::auth::Authorization::SignUpRequired(_)) => {
                Err(SignInError::SignUpRequired)
            }
            Err(err) if err.is("SESSION_PASSWORD_NEEDED") => {
                let password_token = self.get_password_information().await;
                match password_token {
                    Ok(token) => Err(SignInError::PasswordRequired(token)),
                    Err(e) => Err(SignInError::Other(e)),
                }
            }
            Err(err) if err.is("PHONE_CODE_*") => Err(SignInError::InvalidCode),
            Err(error) => Err(SignInError::Other(error)),
        }
    }

    /// Signs out of the account authorized by this client's session.
    ///
    /// If the client was not logged in, this method returns false.
    ///
    /// The client is not disconnected after signing out.
    ///
    /// Note that after using this method you will have to sign in again. If all you want to do
    /// is disconnect, simply [`drop`] the [`Client`] instance.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// if client.sign_out().await.is_ok() {
    ///     println!("Signed out successfully!");
    /// } else {
    ///     println!("No user was signed in, so nothing has changed...");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn sign_out(&self) -> Result<tl::enums::auth::LoggedOut, InvocationError> {
        self.invoke(&tl::functions::auth::LogOut {}).await
    }

    /// Signals all clients sharing the same sender pool to disconnect.
    pub fn disconnect(&self) {
        self.0.handle.quit();
    }
}

#[cfg(test)]
mod tests {
    use mtg_session::Session;
    use mtg_testkit::FakeServer;
    use mtg_tl_types as tl;

    use super::*;
    use crate::test_util::{client, client_with, user};

    fn authorization() -> tl::enums::auth::Authorization {
        tl::types::auth::Authorization {
            setup_password_required: false,
            otherwise_relogin_days: None,
            tmp_sessions: None,
            future_auth_token: None,
            user: user(5),
        }
        .into()
    }

    fn token() -> LoginToken {
        LoginToken {
            phone: "+100".into(),
            phone_code_hash: "hash".into(),
        }
    }

    fn state() -> tl::enums::updates::State {
        tl::types::updates::State {
            pts: 10,
            qts: 11,
            date: 12,
            seq: 13,
            unread_count: 0,
        }
        .into()
    }

    fn sent_code() -> tl::enums::auth::SentCode {
        tl::types::auth::SentCode {
            r#type: tl::types::auth::SentCodeTypeApp { length: 5 }.into(),
            phone_code_hash: "hash".into(),
            next_type: None,
            timeout: None,
        }
        .into()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn authorization_is_known_from_get_state() {
        let server = FakeServer::start().await;
        let c = client(&server);
        server.on(|_: tl::functions::updates::GetState| Ok(state()));
        assert!(c.is_authorized().await.unwrap());
        let server = FakeServer::start().await;
        let c = client(&server);
        server.on(|_: tl::functions::updates::GetState| Err((401, "AUTH_KEY_UNREGISTERED".into())));
        assert!(!c.is_authorized().await.unwrap());
        let server = FakeServer::start().await;
        let c = client(&server);
        server.on(|_: tl::functions::updates::GetState| Err((500, "INTERNAL".into())));
        assert!(c.is_authorized().await.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_bot_signs_in_and_its_state_is_stored() {
        let server = FakeServer::start().await;
        server.on(|req: tl::functions::auth::ImportBotAuthorization| {
            assert_eq!(
                (req.bot_auth_token.as_str(), req.api_hash.as_str()),
                ("tok", "hash")
            );
            Ok(authorization())
        });
        server.on(|_: tl::functions::updates::GetState| Ok(state()));
        let (c, session) = client_with(server.session());
        let me = c.bot_sign_in("tok", "hash").await.unwrap();
        assert_eq!(me.id().bare_id_unchecked(), 5);
        let state = session.updates_state().await.unwrap();
        assert_eq!(
            (state.pts, state.qts, state.date, state.seq),
            (10, 11, 12, 13)
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_failed_get_state_after_login_is_not_fatal() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::auth::ImportBotAuthorization| Ok(authorization()));
        server.on(|_: tl::functions::updates::GetState| Err((500, "INTERNAL".into())));
        let (c, session) = client_with(server.session());
        c.bot_sign_in("tok", "hash").await.unwrap();
        assert_eq!(session.updates_state().await.unwrap().pts, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_bot_login_follows_a_migration_and_refuses_sign_up() {
        let server = FakeServer::start().await;
        let tries = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let t = tries.clone();
        server.on(move |_: tl::functions::auth::ImportBotAuthorization| {
            if t.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                Err((303, "USER_MIGRATE_2".into()))
            } else {
                Ok(tl::enums::auth::Authorization::SignUpRequired(
                    tl::types::auth::AuthorizationSignUpRequired {
                        terms_of_service: None,
                    },
                ))
            }
        });
        let c = client(&server);
        assert!(c.bot_sign_in("tok", "hash").await.is_err());
        assert_eq!(tries.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn other_bot_login_errors_pass_through() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::auth::ImportBotAuthorization| {
            Err((400, "ACCESS_TOKEN_INVALID".into()))
        });
        assert!(client(&server).bot_sign_in("tok", "hash").await.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_login_code_is_requested_and_a_migration_is_followed() {
        let server = FakeServer::start().await;
        let tries = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let t = tries.clone();
        server.on(move |req: tl::functions::auth::SendCode| {
            assert_eq!(req.phone_number, "+100");
            if t.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                Err((303, "PHONE_MIGRATE_2".into()))
            } else {
                Ok(sent_code())
            }
        });
        let token = client(&server)
            .request_login_code("+100", "hash")
            .await
            .unwrap();
        assert_eq!(
            (token.phone.as_str(), token.phone_code_hash.as_str()),
            ("+100", "hash")
        );
        assert_eq!(tries.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unsupported_login_code_answers_are_errors() {
        let success = tl::enums::auth::SentCode::Success(tl::types::auth::SentCodeSuccess {
            authorization: authorization(),
        });
        let payment =
            tl::enums::auth::SentCode::PaymentRequired(tl::types::auth::SentCodePaymentRequired {
                store_product: "p".into(),
                phone_code_hash: "h".into(),
                support_email_address: "a@b.c".into(),
                support_email_subject: "s".into(),
                currency: "USD".into(),
                amount: 1,
                premium_days: 1,
            });
        for answer in [success, payment] {
            // directly and after a migration
            for migrate in [false, true] {
                let server = FakeServer::start().await;
                let first = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(migrate));
                let answer = answer.clone();
                server.on(move |_: tl::functions::auth::SendCode| {
                    if first.swap(false, std::sync::atomic::Ordering::SeqCst) {
                        Err((303, "PHONE_MIGRATE_2".into()))
                    } else {
                        Ok(answer.clone())
                    }
                });
                assert!(client(&server).request_login_code("+1", "h").await.is_err());
            }
        }
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::auth::SendCode| Err((400, "PHONE_NUMBER_INVALID".into())));
        assert!(client(&server).request_login_code("+1", "h").await.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn signing_in_with_a_code_covers_every_outcome() {
        let outcome = |answer: Result<tl::enums::auth::Authorization, (i32, String)>| async move {
            let server = FakeServer::start().await;
            server.on(move |_: tl::functions::auth::SignIn| answer.clone());
            server.on(|_: tl::functions::updates::GetState| Ok(state()));
            server.on(|_: tl::functions::account::GetPassword| {
                Ok(tl::types::account::Password {
                    has_recovery: false,
                    has_secure_values: false,
                    has_password: false,
                    current_algo: None,
                    srp_b: None,
                    srp_id: None,
                    hint: None,
                    email_unconfirmed_pattern: None,
                    new_algo: tl::enums::PasswordKdfAlgo::Unknown,
                    new_secure_algo: tl::enums::SecurePasswordKdfAlgo::Unknown,
                    secure_random: Vec::new(),
                    pending_reset_date: None,
                    login_email_pattern: None,
                }
                .into())
            });
            client(&server).sign_in(&token(), "12345").await
        };
        assert!(outcome(Ok(authorization())).await.is_ok());
        let sign_up = tl::enums::auth::Authorization::SignUpRequired(
            tl::types::auth::AuthorizationSignUpRequired {
                terms_of_service: None,
            },
        );
        assert!(matches!(
            outcome(Ok(sign_up)).await,
            Err(SignInError::SignUpRequired)
        ));
        assert!(matches!(
            outcome(Err((401, "SESSION_PASSWORD_NEEDED".into()))).await,
            Err(SignInError::PasswordRequired(_))
        ));
        assert!(matches!(
            outcome(Err((400, "PHONE_CODE_INVALID".into()))).await,
            Err(SignInError::InvalidCode)
        ));
        assert!(matches!(
            outcome(Err((500, "INTERNAL".into()))).await,
            Err(SignInError::Other(_))
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_failing_password_lookup_is_reported() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::auth::SignIn| Err((401, "SESSION_PASSWORD_NEEDED".into())));
        server.on(|_: tl::functions::account::GetPassword| Err((500, "INTERNAL".into())));
        let token = LoginToken {
            phone: "+1".into(),
            phone_code_hash: "h".into(),
        };
        assert!(matches!(
            client(&server).sign_in(&token, "1").await,
            Err(SignInError::Other(_))
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn signing_out_and_disconnecting() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::auth::LogOut| {
            Ok(tl::types::auth::LoggedOut {
                future_auth_token: None,
            }
            .into())
        });
        let c = client(&server);
        c.sign_out().await.unwrap();
        c.disconnect();
    }
}
