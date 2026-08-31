#[cfg(test)]
mod tests_auth {
    use base64::Engine;
    use base64::prelude::BASE64_STANDARD;
    use tokio::net::TcpStream;
    use crate::smtp::smtp::Smtp2;
    use crate::tests::test::expect_msg;
    use crate::tests::test::test::{ehlo_msg, test_setup};

    fn lf(s: &str) -> String {
        let mut r = s.to_string();
        r.push_str("\r\n");
        r
    }

    #[tokio::test]
    async fn test_auth_unknown_mech() {
        let mut s: Smtp2<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle(lf("EHLO test.org")).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        (s, _) = s.handle(lf("AUTH CRAM-MD5")).await.unwrap();
        expect_msg!(s, "535 5.7.8 Invalid authentication mechanism\r\n");
    }

    #[tokio::test]
    async fn test_auth_no_mech() {
        let mut s: Smtp2<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle(lf("EHLO test.org")).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        (s, _) = s.handle(lf("AUTH")).await.unwrap();
        expect_msg!(s, "535 5.7.8 Invalid authentication mechanism\r\n");
    }

    #[tokio::test]
    async fn test_auth_plain_oneline_success() {
        let mut s: Smtp2<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle(lf("EHLO test.org")).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        s.user_db().lock().unwrap().mock_user("spongebob", "pineapple!");

        (s, _) = s.handle(lf("AUTH PLAIN spongebob\0spongebob\0pineapple!")).await.unwrap();
        expect_msg!(s, "235 2.7.0 Authentication successful\r\n");
    }

    // todo figure out whether this should be possible
    //#[tokio::test]
    async fn test_auth_plain_multiline_success() {
        let mut s: Smtp2<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle(lf("EHLO test.org")).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        (s, _) = s.handle(lf("AUTH PLAIN")).await.unwrap();
        expect_msg!(s, "334 \r\n");

        s.user_db().lock().unwrap().mock_user("spongebob", "pineapple!");

        (s, _) = s.handle(lf("spongebob\0spongebob\0pineapple!")).await.unwrap();
        expect_msg!(s, "235 2.7.0 Authentication successful\r\n");
    }

    #[tokio::test]
    async fn test_auth_plain_oneline64_success() {
        let mut s: Smtp2<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle(lf("EHLO test.org")).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        s.user_db().lock().unwrap().mock_user("spongebob", "pineapple!");

        (s, _) = s.handle(format!(
            "AUTH PLAIN {}\r\n",
            BASE64_STANDARD.encode("spongebob\0spongebob\0pineapple!"))).await.unwrap();
        expect_msg!(s, "235 2.7.0 Authentication successful\r\n");
    }

    #[tokio::test]
    async fn test_auth_plain_multiline64_success() {
        let mut s: Smtp2<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle(lf("EHLO test.org")).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        s.user_db().lock().unwrap().mock_user("spongebob", "pineapple!");

        let credentials = BASE64_STANDARD.encode("spongebob\0spongebob\0pineapple!");
        (s, _) = s.handle(format!("AUTH PLAIN {}", credentials)).await.unwrap();
        expect_msg!(s, "235 2.7.0 Authentication successful\r\n");
    }

    #[tokio::test]
    async fn test_auth_plain_no_identity() {
        let mut s: Smtp2<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle(lf("EHLO test.org")).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        s.user_db().lock().unwrap().mock_user("spongebob", "pineapple!");

        (s, _) = s.handle(lf("AUTH PLAIN \0spongebob\0pineapple!")).await.unwrap();
        expect_msg!(s, "235 2.7.0 Authentication successful\r\n");
    }

    #[tokio::test]
    async fn test_auth_plain_wrong_pw() {
        let mut s: Smtp2<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle(lf("EHLO test.org")).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));
        
        s.user_db().lock().unwrap().mock_user("spongebob", "pineapple!");

        (s, _) = s.handle(lf("AUTH PLAIN spongebob\0spongebob\0wrongpw")).await.unwrap();
        expect_msg!(s, "535 5.7.8 Unauthorized\r\n");
    }

    #[tokio::test]
    async fn test_auth_login_success() {
        let mut s: Smtp2<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle(lf("EHLO test.org\r\n")).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        (s, _) = s.handle(lf("AUTH LOGIN")).await.unwrap();
        expect_msg!(s, "334 VXNlciBOYW1lAA==\r\n");

        s.user_db().lock().unwrap().mock_user("spongebob", "pineapple!");

        (s, _) = s.handle(lf(BASE64_STANDARD.encode(b"spongebob").as_ref())).await.unwrap();
        expect_msg!(s, "334 UGFzc3dvcmQA\r\n");
        
        (s, _) = s.handle(lf(BASE64_STANDARD.encode(b"pineapple!").as_ref())).await.unwrap();
        expect_msg!(s, "235 2.7.0 Authentication successful\r\n");
    }

    #[tokio::test]
    async fn test_auth_lowercase_mech() {
        let mut s: Smtp2<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle(lf("EHLO test.org")).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        (s, _) = s.handle(lf("AUTH login")).await.unwrap();
        expect_msg!(s, "334 VXNlciBOYW1lAA==\r\n");
    }

    #[tokio::test]
    async fn test_auth_invalid_mech_arg() {
        let mut s: Smtp2<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle(lf("EHLO test.org")).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        (s, _) = s.handle(lf("AUTH MECH&!ARG")).await.unwrap();
        expect_msg!(s, "535 5.7.8 Invalid authentication mechanism\r\n");
    }

    #[tokio::test]
    async fn test_auth_login_invalid_input() {
        let mut s: Smtp2<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle(lf("EHLO test.org")).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        (s, _) = s.handle(lf("AUTH LOGIN")).await.unwrap();
        expect_msg!(s, "334 VXNlciBOYW1lAA==\r\n");

        s.user_db().lock().unwrap().mock_user("spongebob", "pineapple!");

        (s, _) = s.handle(lf("spongebob\0")).await.unwrap();
        expect_msg!(s, "535 5.7.8 Unauthorized\r\n");
    }
}
