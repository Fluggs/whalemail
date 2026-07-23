#[cfg(test)]
mod tests_auth {
    use base64::Engine;
    use base64::prelude::BASE64_STANDARD;
    use crate::tests::test::expect_msg;
    use crate::tests::test::test::{test_setup, EHLO_MSG};
    
    fn lf(s: &str) -> String {
        let mut r = s.to_string();
        r.push_str("\r\n");
        r
    }

    #[tokio::test]
    async fn test_auth_unknown_mech() {
        let mut s = test_setup();
        s.init_smtp().await.unwrap();
        expect_msg!(s, "220 hi\r\n");

        s.handle(lf("EHLO test.org")).await.unwrap();
        expect_msg!(s, EHLO_MSG);

        s.handle(lf("AUTH CRAM-MD5")).await.unwrap();
        expect_msg!(s, "535 5.7.8 Invalid authentication mechanism\r\n");
    }

    #[tokio::test]
    async fn test_auth_no_mech() {
        let mut s = test_setup();
        s.init_smtp().await.unwrap();
        expect_msg!(s, "220 hi\r\n");

        s.handle(lf("EHLO test.org")).await.unwrap();
        expect_msg!(s, EHLO_MSG);

        s.handle(lf("AUTH")).await.unwrap();
        expect_msg!(s, "535 5.7.8 Invalid authentication mechanism\r\n");
    }

    #[tokio::test]
    async fn test_auth_plain_success() {
        let mut s = test_setup();
        s.init_smtp().await.unwrap();
        expect_msg!(s, "220 hi\r\n");

        s.handle(lf("EHLO test.org")).await.unwrap();
        expect_msg!(s, EHLO_MSG);

        s.handle(lf("AUTH PLAIN")).await.unwrap();
        expect_msg!(s, "334 \r\n");

        s.user_db.lock().unwrap().mock("spongebob".to_string(), "pineapple!".to_string());

        s.handle(lf("spongebob\0spongebob\0pineapple!")).await.unwrap();
        expect_msg!(s, "235 2.7.0 Authentication successful\r\n");
    }

    #[tokio::test]
    async fn test_auth_plain_wrong_pw() {
        let mut s = test_setup();
        s.init_smtp().await.unwrap();
        expect_msg!(s, "220 hi\r\n");

        s.handle(lf("EHLO test.org")).await.unwrap();
        expect_msg!(s, EHLO_MSG);

        s.handle(lf("AUTH PLAIN")).await.unwrap();
        expect_msg!(s, "334 \r\n");
        
        s.user_db.lock().unwrap().mock("spongebob".to_string(), "pineapple!".to_string());

        s.handle(lf("spongebob\0spongebob\0wrongpw")).await.unwrap();
        expect_msg!(s, "535 5.7.8 Unauthorized\r\n");
    }

    #[tokio::test]
    async fn test_auth_login_success() {
        let mut s = test_setup();
        s.init_smtp().await.unwrap();
        expect_msg!(s, "220 hi\r\n");

        s.handle(lf("EHLO test.org\r\n")).await.unwrap();
        expect_msg!(s, EHLO_MSG);

        s.handle(lf("AUTH LOGIN")).await.unwrap();
        expect_msg!(s, "334 VXNlciBOYW1lAA==\r\n");

        s.user_db.lock().unwrap().mock("spongebob".to_string(), "pineapple!".to_string());

        s.handle(lf(BASE64_STANDARD.encode(b"spongebob").as_ref())).await.unwrap();
        expect_msg!(s, "334 UGFzc3dvcmQA\r\n");
        
        s.handle(lf(BASE64_STANDARD.encode(b"pineapple!").as_ref())).await.unwrap();
        expect_msg!(s, "235 2.7.0 Authentication successful\r\n");
    }

    #[tokio::test]
    async fn test_auth_login_invalid_input() {
        let mut s = test_setup();
        s.init_smtp().await.unwrap();
        expect_msg!(s, "220 hi\r\n");

        s.handle(lf("EHLO test.org")).await.unwrap();
        expect_msg!(s, EHLO_MSG);

        s.handle(lf("AUTH LOGIN")).await.unwrap();
        expect_msg!(s, "334 VXNlciBOYW1lAA==\r\n");

        s.user_db.lock().unwrap().mock("spongebob".to_string(), "pineapple!".to_string());

        s.handle(lf("spongebob\0")).await.unwrap();
        expect_msg!(s, "535 5.7.8 Unauthorized\r\n");
    }
}