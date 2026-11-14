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
        let current_algo = password_info.current_algo.clone().unwrap();
        let mut params = utils::extract_password_parameters(&current_algo);

        // Telegram sent us incorrect parameters, trying to get them again
        if !check_p_and_g(params.2, params.3) {
            password_info = self
                .get_password_information()
                .await
                .map_err(SignInError::Other)?
                .password;
            params =
                utils::extract_password_parameters(password_info.current_algo.as_ref().unwrap());
            if !check_p_and_g(params.2, params.3) {
                panic!("Failed to get correct password information from Telegram")
            }
        }

        let (salt1, salt2, p, g) = params;

        let g_b = password_info.srp_b.clone().unwrap();
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
                srp_id: password_info.srp_id.unwrap(),
                a: g_a.to_vec(),
                m1: m1.to_vec(),
            }),
        };

        match self.invoke(&check_password).await {
            Ok(tl::enums::auth::Authorization::Authorization(x)) => {
                self.complete_login(x).await.map_err(SignInError::Other)
            }
            Ok(tl::enums::auth::Authorization::SignUpRequired(_x)) => panic!("Unexpected result"),
            Err(err) if err.is("PASSWORD_HASH_INVALID") => {
                Err(SignInError::InvalidPassword(PasswordToken {
                    password: password_info,
                }))
            }
            Err(error) => Err(SignInError::Other(error)),
        }
    }
}
