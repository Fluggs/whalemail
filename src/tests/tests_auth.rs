#[cfg(test)]
mod tests_auth {
    use crate::tests::test::expect_msg;
    use crate::tests::test::test::test_setup;

    #[tokio::test]
    async fn test_auth_success() {
        let mut s = test_setup();
        s.init_smtp().await.unwrap();
        expect_msg!(s, "220 hi\r\n");

        s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250-AUTH PLAIN\r\n");

        s.handle("AUTH PLAIN\r\n".to_string()).await.unwrap();
        expect_msg!(s, "334 \r\n");

        s.user_db.lock().unwrap().mock("spongebob".to_string(), "pineapple!".to_string());

        s.handle("spongebob\0spongebob\0pineapple!".to_string()).await.unwrap();
        expect_msg!(s, "235 2.7.0 Authentication successful\r\n");
    }

    #[tokio::test]
    async fn test_auth_wrong_pw() {
        let mut s = test_setup();
        s.init_smtp().await.unwrap();
        expect_msg!(s, "220 hi\r\n");

        s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250-AUTH PLAIN\r\n");

        s.handle("AUTH PLAIN\r\n".to_string()).await.unwrap();
        expect_msg!(s, "334 \r\n");
        
        s.user_db.lock().unwrap().mock("spongebob".to_string(), "pineapple!".to_string());

        s.handle("spongebob\0spongebob\0wrongpw".to_string()).await.unwrap();
        expect_msg!(s, "535 Unauthorized\r\n");
    }

    #[tokio::test]
    async fn test_auth_unknown_mech() {
        let mut s = test_setup();
        s.init_smtp().await.unwrap();
        expect_msg!(s, "220 hi\r\n");

        s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250-AUTH PLAIN\r\n");

        s.handle("AUTH LOGIN\r\n".to_string()).await.unwrap();
        expect_msg!(s, "504 Bad parameter\r\n");
    }

    #[tokio::test]
    async fn test_auth_no_mech() {
        let mut s = test_setup();
        s.init_smtp().await.unwrap();
        expect_msg!(s, "220 hi\r\n");

        s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250-AUTH PLAIN\r\n");

        s.handle("AUTH\r\n".to_string()).await.unwrap();
        expect_msg!(s, "504 Bad parameter\r\n");
    }
}