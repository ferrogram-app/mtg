use super::*;

impl Client {
    /// Extract information needed for the two-factor authentication
    /// It's called automatically when we get SESSION_PASSWORD_NEEDED error during sign in.
    pub(super) async fn get_password_information(&self) -> Result<PasswordToken, InvocationError> {
        let request = tl::functions::account::GetPassword {};

        let password: tl::types::account::Password = self.invoke(&request).await?.into();

        Ok(PasswordToken::new(password))
    }

    /// Sign in using two-factor authentication (user password).
    ///
    /// [`PasswordToken`] can be obtained from [`SignInError::PasswordRequired`] error after the
    /// [`Client::sign_in`] method fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use mtg_client::SignInError;
    ///
    /// # async fn f(client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// # const API_HASH: &str = "";
    /// # const PHONE: &str = "";
    /// fn get_user_password(hint: &str) -> Vec<u8> {
    ///     unimplemented!()
    /// }
    ///
    /// # let token = client.request_login_code(PHONE, API_HASH).await?;
    /// # let code = "";
    ///
    /// // ... enter phone number, request login code ...
    ///
    /// let user = match client.sign_in(&token, &code).await {
    ///     Err(SignInError::PasswordRequired(password_token) ) => {
    ///         let mut password = get_user_password(password_token.hint().unwrap());
    ///
    ///         client
    ///             .check_password(password_token, password)
    ///             .await.unwrap()
    ///     }
    ///     Ok(user) => user,
    ///     Ok(_) => panic!("Sign in required"),
    ///     Err(err) => {
    ///         panic!("Failed to sign in as a user :(\n{err}");
    ///     }
    /// };
    /// # Ok(())
    /// # }
    /// ```
    pub async fn check_password(
        &self,
        password_token: PasswordToken,
        password: impl AsRef<[u8]>,
    ) -> Result<User, SignInError> {
        let mut password_info = password_token.password;
        let bad_srp = |what: &'static str| {
            SignInError::Other(InvocationError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                what,
            )))
        };
        let current_algo = password_info
            .current_algo
            .clone()
            .ok_or_else(|| bad_srp("password info without a KDF algorithm"))?;
        let mut params = utils::extract_password_parameters(&current_algo)
            .ok_or_else(|| bad_srp("unsupported password KDF algorithm"))?;

        // Telegram sent us incorrect parameters, trying to get them again
        if !check_p_and_g(params.2, params.3) {
            password_info = self
                .get_password_information()
                .await
                .map_err(SignInError::Other)?
                .password;
            params = password_info
                .current_algo
                .as_ref()
                .and_then(utils::extract_password_parameters)
                .ok_or_else(|| bad_srp("unsupported password KDF algorithm"))?;
            if !check_p_and_g(params.2, params.3) {
                return Err(bad_srp("Telegram sent incorrect password parameters twice"));
            }
        }

        let (salt1, salt2, p, g) = params;

        let g_b = password_info
            .srp_b
            .clone()
            .ok_or_else(|| bad_srp("password info without srp_b"))?;
        let a = password_info.secure_random.clone();

        // None: the server sent a g_b that is out of range
        let (m1, g_a) = calculate_2fa(salt1, salt2, p, g, g_b, a, password).ok_or_else(|| {
            SignInError::Other(InvocationError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid SRP parameters from the server",
            )))
        })?;

        let check_password = tl::functions::auth::CheckPassword {
            password: tl::enums::InputCheckPasswordSrp::Srp(tl::types::InputCheckPasswordSrp {
                srp_id: password_info
                    .srp_id
                    .ok_or_else(|| bad_srp("password info without srp_id"))?,
                a: g_a.to_vec(),
                m1: m1.to_vec(),
            }),
        };

        match self.invoke(&check_password).await {
            Ok(tl::enums::auth::Authorization::Authorization(x)) => {
                self.complete_login(x).await.map_err(SignInError::Other)
            }
            Ok(tl::enums::auth::Authorization::SignUpRequired(_)) => {
                Err(bad_srp("unexpected SignUpRequired after a password check"))
            }
            Err(err) if err.is("PASSWORD_HASH_INVALID") => {
                Err(SignInError::InvalidPassword(PasswordToken {
                    password: password_info,
                }))
            }
            Err(error) => Err(SignInError::Other(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use mtg_testkit::FakeServer;

    use super::*;
    use crate::test_util::{client, user};

    // RFC 3526 group 14: a 2048-bit safe prime.
    const P14: &str = "ffffffffffffffffc90fdaa22168c234c4c6628b80dc1cd129024e088a67cc74020bbea63b139b22514a08798e3404ddef9519b3cd3a431b302b0a6df25f14374fe1356d6d51c245e485b576625e7ec6f44c42e9a637ed6b0bff5cb6f406b7edee386bfb5a899fa5ae9f24117c4b1fe649286651ece45b3dc2007cb8a163bf0598da48361c55d39a69163fa8fd24cf5f83655d23dca3ad961c62f356208552bb9ed529077096966d670c354e4abc9804f1746c08ca18217c32905e462e36ce3be39e772c180e86039b2783a2ec07a28fb5c55df06f4c52c9de2bcbf6955817183995497cea956ae515d2261898fa051015728e5a8aacaa68ffffffffffffffff";

    fn p14() -> Vec<u8> {
        (0..P14.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&P14[i..i + 2], 16).unwrap())
            .collect()
    }

    fn algo(p: Vec<u8>) -> tl::enums::PasswordKdfAlgo {
        tl::types::PasswordKdfAlgoSha256Sha256Pbkdf2Hmacsha512iter100000Sha256ModPow {
            salt1: vec![1],
            salt2: vec![2],
            g: 2,
            p,
        }
        .into()
    }

    fn password(
        algo: Option<tl::enums::PasswordKdfAlgo>,
        srp_b: Option<Vec<u8>>,
        srp_id: Option<i64>,
    ) -> tl::types::account::Password {
        tl::types::account::Password {
            has_recovery: false,
            has_secure_values: false,
            has_password: true,
            current_algo: algo,
            srp_b,
            srp_id,
            hint: Some("hint".into()),
            email_unconfirmed_pattern: None,
            new_algo: tl::enums::PasswordKdfAlgo::Unknown,
            new_secure_algo: tl::enums::SecurePasswordKdfAlgo::Unknown,
            secure_random: vec![7; 8],
            pending_reset_date: None,
            login_email_pattern: None,
        }
    }

    fn good() -> tl::types::account::Password {
        password(Some(algo(p14())), Some(vec![5]), Some(9))
    }

    async fn check(
        server: &FakeServer,
        pw: tl::types::account::Password,
    ) -> Result<User, SignInError> {
        client(server)
            .check_password(PasswordToken::new(pw), "secret")
            .await
    }

    fn is_other(e: Result<User, SignInError>, what: &str) -> bool {
        matches!(e, Err(SignInError::Other(e)) if e.to_string().contains(what))
    }

    #[test]
    fn the_token_shows_the_hint() {
        assert_eq!(PasswordToken::new(good()).hint(), Some("hint"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn password_information_is_fetched() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::account::GetPassword| {
            Ok(tl::enums::account::Password::from(good()))
        });
        let token = client(&server).get_password_information().await.unwrap();
        assert_eq!(token.hint(), Some("hint"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn missing_or_unknown_algorithms_are_errors() {
        let server = FakeServer::start().await;
        let r = check(&server, password(None, Some(vec![5]), Some(9))).await;
        assert!(is_other(r, "without a KDF algorithm"));
        let r = check(
            &server,
            password(Some(tl::enums::PasswordKdfAlgo::Unknown), None, None),
        )
        .await;
        assert!(is_other(r, "unsupported password KDF"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn bad_parameters_are_refetched_once() {
        let server = FakeServer::start().await;
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        server.on(move |_: tl::functions::account::GetPassword| {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(tl::enums::account::Password::from(password(
                Some(algo(vec![0xff; 256])),
                Some(vec![5]),
                Some(9),
            )))
        });
        let bad = password(Some(algo(vec![0xff; 256])), Some(vec![5]), Some(9));
        assert!(is_other(
            check(&server, bad).await,
            "incorrect password parameters twice"
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn refetched_parameters_without_an_algorithm_are_an_error() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::account::GetPassword| {
            // no current_algo: the password flag must be off to serialize
            let mut p = password(None, None, None);
            p.has_password = false;
            Ok(tl::enums::account::Password::from(p))
        });
        let bad = password(Some(algo(vec![0xff; 256])), Some(vec![5]), Some(9));
        assert!(is_other(
            check(&server, bad).await,
            "unsupported password KDF"
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn failing_refetch_is_reported() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::account::GetPassword| {
            Err::<tl::enums::account::Password, _>((400, "PASSWORD_EMPTY".into()))
        });
        let bad = password(Some(algo(vec![0xff; 256])), Some(vec![5]), Some(9));
        assert!(is_other(check(&server, bad).await, "PASSWORD_EMPTY"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn good_refetched_parameters_are_used() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::account::GetPassword| {
            Ok(tl::enums::account::Password::from(good()))
        });
        server.on(|_: tl::functions::auth::CheckPassword| {
            Err::<tl::enums::auth::Authorization, _>((400, "PASSWORD_HASH_INVALID".into()))
        });
        let bad = password(Some(algo(vec![0xff; 256])), Some(vec![5]), Some(9));
        assert!(matches!(
            check(&server, bad).await,
            Err(SignInError::InvalidPassword(_))
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn incomplete_or_hostile_srp_data_is_an_error() {
        let server = FakeServer::start().await;
        let r = check(&server, password(Some(algo(p14())), None, Some(9))).await;
        assert!(is_other(r, "without srp_b"));
        // g_b of 1 is out of range
        let r = check(&server, password(Some(algo(p14())), Some(vec![1]), Some(9))).await;
        assert!(is_other(r, "invalid SRP parameters"));
        let r = check(&server, password(Some(algo(p14())), Some(vec![5]), None)).await;
        assert!(is_other(r, "without srp_id"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_server_verdict_decides() {
        let server = FakeServer::start().await;
        let answer = Arc::new(std::sync::Mutex::new(0));
        let a = answer.clone();
        server.on(move |req: tl::functions::auth::CheckPassword| {
            let tl::enums::InputCheckPasswordSrp::Srp(s) = req.password else {
                panic!()
            };
            assert_eq!(s.srp_id, 9);
            assert_eq!((s.a.len(), s.m1.len()), (256, 32));
            match *a.lock().unwrap() {
                0 => Ok(tl::enums::auth::Authorization::from(
                    tl::types::auth::Authorization {
                        setup_password_required: false,
                        otherwise_relogin_days: None,
                        tmp_sessions: None,
                        future_auth_token: None,
                        user: user(5),
                    },
                )),
                1 => Ok(tl::types::auth::AuthorizationSignUpRequired {
                    terms_of_service: None,
                }
                .into()),
                _ => Err((500, "INTERNAL".into())),
            }
        });
        assert!(check(&server, good()).await.is_ok());
        *answer.lock().unwrap() = 1;
        assert!(is_other(check(&server, good()).await, "SignUpRequired"));
        *answer.lock().unwrap() = 2;
        assert!(matches!(
            check(&server, good()).await,
            Err(SignInError::Other(_))
        ));
    }
}
