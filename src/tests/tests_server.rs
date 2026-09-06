#[cfg(test)]
mod tests_smtp {
    use base64::Engine;
    use base64::prelude::BASE64_STANDARD;
    use tokio::net::TcpStream;
    use crate::smtp::server::{DataState, SmtpServer, StateKind};
    use crate::smtp::envelope::MailAddress;
    use crate::tests::test::expect_msg;
    use crate::tests::test::test::{ehlo_msg, test_setup};

    fn lf(s: &str) -> String {
        let mut r = s.to_string();
        r.push_str("\r\n");
        r
    }

    #[tokio::test]
    async fn test_init() {
        let mut s: SmtpServer<TcpStream> = test_setup().await;

        expect_msg!(s, "220 hi\r\n");
    }

    #[tokio::test]
    async fn test_n_unknown_cmd() {
        let mut s: SmtpServer<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle("blub\r\n".to_string()).await.unwrap();
        expect_msg!(s, "500 Unrecognized command\r\n");
    }

    #[tokio::test]
    async fn test_helo_mail() {
        let sender = "sender@test.org";
        let sender_addr = MailAddress::new(sender).unwrap();
        let rcpt = MailAddress::new("rcv@whalemail.net").unwrap();

        let mut s: SmtpServer<TcpStream> = test_setup().await;
        s.user_db().lock().unwrap().mock_mailbox(MailAddress::new("rcv@whalemail.net").unwrap());
        
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle("MAIL FROM:<".to_string() + sender + ">\r\n").await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle(format!("RCPT TO:<{}>\r\n", rcpt.address.as_str())).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle("DATA\r\n".to_string()).await.unwrap();
        expect_msg!(s, "354 start mail input\r\n");

        (s, _) = s.handle("<mailblob> blob blob\r\n.\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        let r;
        (s, r) = s.handle("QUIT\r\n".to_string()).await.unwrap();
        expect_msg!(s, "221 closing channel\r\n");
        assert_eq!(r, StateKind::QUIT);

        // Verify mail
        assert_eq!(s.mail().sender.address, sender_addr.address);
        assert_eq!(s.mail().recipients, Vec::from([rcpt]));
    }

    #[tokio::test]
    async fn test_ehlo_mail() {
        let sender = "sender@test.org";
        let sender_addr = MailAddress::new(sender).unwrap();
        let rcpt = MailAddress::new("rcv@whalemail.net").unwrap();

        let mut s: SmtpServer<TcpStream> = test_setup().await;
        s.user_db().lock().unwrap().mock_mailbox(MailAddress::new("rcv@whalemail.net").unwrap());
        
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        (s, _) = s.handle("MAIL FROM:<".to_string() + sender + ">\r\n").await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle(format!("RCPT TO:<{}>\r\n", rcpt.address.as_str())).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle("DATA\r\n".to_string()).await.unwrap();
        expect_msg!(s, "354 start mail input\r\n");

        (s, _) = s.handle("<mailblob> blob blob\r\n.\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        let r;
        (s, r) = s.handle("QUIT\r\n".to_string()).await.unwrap();
        expect_msg!(s, "221 closing channel\r\n");
        assert_eq!(r, StateKind::QUIT);

        // Verify mail
        assert_eq!(s.mail().sender.address, sender_addr.address);
        assert_eq!(s.mail().recipients, Vec::from([rcpt]));
    }

    #[tokio::test]
    async fn test_n_mail_parts() {
        let mailct_1 = "<mailblob> blob blob\r\n".to_string();
        let mailct_2 = "more blob\r\n.\r\n".to_string();
        let mut s: SmtpServer<TcpStream> = test_setup().await;
        s.user_db().lock().unwrap().mock_mailbox(MailAddress::new("rcv@whalemail.net").unwrap());
        
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle("MAIL FROM:<sender@test.org>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle("RCPT TO:<rcv@whalemail.net>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle("DATA\r\n".to_string()).await.unwrap();
        expect_msg!(s, "354 start mail input\r\n");

        (s, _) = s.handle(mailct_1.clone()).await.unwrap();
        s.expect_no_msg();

        (s, _) = s.handle(mailct_2.clone()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        let r;
        (s, r) = s.handle("QUIT\r\n".to_string()).await.unwrap();
        expect_msg!(s, "221 closing channel\r\n");
        assert_eq!(r, StateKind::QUIT);

        // verify msg
        assert_eq!(s.mail().body, mailct_1 + &mailct_2);
    }

    #[tokio::test]
    async fn test_helo_multiple_rcpt() {
        let rcpt1 = MailAddress::new("rcv1@whalemail.net").unwrap();
        let rcpt2 = MailAddress::new("rcv2@whalemail.net").unwrap();
        let mut s: SmtpServer<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle("MAIL FROM:<sender@test.org>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.user_db().lock().unwrap().mock_mailbox(rcpt1.clone());
        (s, _) = s.handle(format!("RCPT TO:<{}>\r\n", rcpt1.address.as_str())).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.user_db().lock().unwrap().mock_mailbox(rcpt2.clone());
        (s, _) = s.handle(format!("RCPT TO:<{}>\r\n", rcpt2.address.as_str())).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        assert_eq!(s.recipients(), &Vec::from([rcpt1, rcpt2]));
    }

    #[tokio::test]
    async fn test_n_omit_rcpt() {
        let mut s: SmtpServer<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle("MAIL FROM:<sender@test.org>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle("DATA\r\n".to_string()).await.unwrap();
        expect_msg!(s, "503 Bad sequence\r\n");
    }

    #[tokio::test]
    async fn test_mailfrom_case_success() {
        let mut s: SmtpServer<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        (s, _) = s.handle("MAIL FROM:<sender@test.org>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");
    }

    #[tokio::test]
    async fn test_mailfrom_bad_command() {
        let mut s: SmtpServer<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        (s, _) = s.handle("MAIL FROM: <sender@test.org>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "500 Unrecognized command\r\n");
    }

    #[tokio::test]
    async fn test_mailfrom_authorized_onestep() {
        let mut s: SmtpServer<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        s.user_db().lock().unwrap().mock_user("sender", "pineapple!");

        (s, _) = s.handle("AUTH PLAIN sender\0sender\0pineapple!\r\n".to_string()).await.unwrap();
        expect_msg!(s, "235 2.7.0 Authentication successful\r\n");

        s.user_db().lock().unwrap().mock_mailbox(MailAddress::new("sender@whalemail.net").unwrap());
        (s, _) = s.handle("MAIL FROM:<sender@whalemail.net>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");
        assert!(s.is_local_sender());
    }

    #[tokio::test]
    async fn test_mailfrom_authorized_multistep() {
        let mut s: SmtpServer<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        s.user_db().lock().unwrap().mock_user("sender", "pineapple!");
        
        (s, _) = s.handle(lf("AUTH LOGIN")).await.unwrap();
        expect_msg!(s, "334 VXNlciBOYW1lAA==\r\n");

        s.user_db().lock().unwrap().mock_user("sender", "pineapple!");

        (s, _) = s.handle(lf(BASE64_STANDARD.encode(b"sender").as_ref())).await.unwrap();
        expect_msg!(s, "334 UGFzc3dvcmQA\r\n");

        (s, _) = s.handle(lf(BASE64_STANDARD.encode(b"pineapple!").as_ref())).await.unwrap();
        expect_msg!(s, "235 2.7.0 Authentication successful\r\n");

        s.user_db().lock().unwrap().mock_mailbox(MailAddress::new("sender@whalemail.net").unwrap());
        (s, _) = s.handle("MAIL FROM:<sender@whalemail.net>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");
        assert!(s.is_local_sender());
    }

    #[tokio::test]
    async fn test_mailfrom_invalid_mailbox() {
        let mut s: SmtpServer<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        (s, _) = s.handle("MAIL FROM:<fsdfdsf>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "450 Invalid host\r\n");
    }

    #[tokio::test]
    async fn test_mailfrom_case_insensitive() {
        let mut s: SmtpServer<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        (s, _) = s.handle("MAIL From:<sender@test.org>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");
    }
    
    #[tokio::test]
    async fn test_mailfrom_not_authenticated() {
        let mut s: SmtpServer<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        s.user_db().lock().unwrap().mock_mailbox(MailAddress::new("sender@whalemail.net").unwrap());
        (s, _) = s.handle("MAIL FROM:<sender@whalemail.net>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "530 5.7.0 Authentication required\r\n");
    }

    #[tokio::test]
    async fn test_mailfrom_unauthorized() {
        let mut s: SmtpServer<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        s.user_db().lock().unwrap().mock_user("anothersender", "pineapple!");

        (s, _) = s.handle("AUTH PLAIN anothersender\0anothersender\0pineapple!\r\n".to_string()).await.unwrap();
        expect_msg!(s, "235 2.7.0 Authentication successful\r\n");

        s.user_db().lock().unwrap().mock_mailbox(MailAddress::new("sender@whalemail.net").unwrap());
        (s, _) = s.handle("MAIL FROM:<sender@whalemail.net>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "530 5.7.0 Authentication required\r\n");
    }

    #[tokio::test]
    async fn test_rcpt_invalid_mailbox() {
        let mut s: SmtpServer<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        (s, _) = s.handle("MAIL FROM:<sender@test.org>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle("RCPT TO:<sadfsd>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "450 Invalid mailbox\r\n");
    }

    #[tokio::test]
    async fn test_rcpt_not_authenticated() {
        let mut s: SmtpServer<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        s.user_db().lock().unwrap().mock_mailbox(MailAddress::new("sender@whalemail.net").unwrap());
        (s, _) = s.handle("MAIL FROM:<sender@test.org>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");
        
        (s, _) = s.handle("RCPT TO:<mail@test.org>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "530 5.7.0 Authentication required\r\n");
    }

    #[tokio::test]
    async fn test_rcpt_bad_command() {
        let mut s: SmtpServer<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        (s, _) = s.handle("MAIL FROM:<sender@test.org>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle("RCPT TO: <mail@test.org>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "500 Unrecognized command\r\n");
    }

    #[tokio::test]
    async fn test_rcpt_case_insensitive() {
        let sender = "sender@test.org";
        let sender_addr = MailAddress::new(sender).unwrap();
        let rcpt = MailAddress::new("rcv@whalemail.net").unwrap();

        let mut s: SmtpServer<TcpStream> = test_setup().await;
        s.user_db().lock().unwrap().mock_mailbox(MailAddress::new("rcv@whalemail.net").unwrap());

        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        (s, _) = s.handle("MAIL FROM:<".to_string() + sender + ">\r\n").await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle(format!("Rcpt To:<{}>\r\n", rcpt.address.as_str())).await.unwrap();
        expect_msg!(s, "250 OK\r\n");
    }

    #[tokio::test]
    async fn test_mail_delivery_error() {
        let sender = "sender@test.org";
        let sender_addr = MailAddress::new(sender).unwrap();
        let rcpt = MailAddress::new("rcv@whalemail.net").unwrap();
        let another_rcpt = MailAddress::new("another_rcv@whalemail.net").unwrap();

        let mut s: SmtpServer<TcpStream> = test_setup().await;
        s.user_db().lock().unwrap().mock_user("rcv", "");

        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        (s, _) = s.handle("MAIL FROM:<".to_string() + sender + ">\r\n").await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.user_db().lock().unwrap().mock_mailbox(rcpt.clone());
        (s, _) = s.handle(format!("RCPT TO:<{}>\r\n", rcpt.address.as_str())).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle("DATA\r\n".to_string()).await.unwrap();
        expect_msg!(s, "354 start mail input\r\n");

        s.user_db().lock().unwrap().mock_mailbox(another_rcpt);
        (s, _) = s.handle("<mailblob> blob blob\r\n.\r\n".to_string()).await.unwrap();
        expect_msg!(s, "450 Requested mail action not taken: mailbox unavailable\r\n");
    }

    /*
    Tests for decode_transparency
     */
    #[test]
    fn test_dtp_empty_s() {
        let expected = "";
        let result = DataState::decode_transparency(expected).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_dtp_simple_mail() {
        let expected = "blub\r\n.\r\n";
        
        let result = DataState::decode_transparency(expected).unwrap();
        assert_eq!(result, expected);
    }

    //#[test]
    /*
        Interpretation of dot-stuffing. RFC says "\r\n.\r\n ends a mail", which
        does not specify whether ^.\r\n (with ^ beginning of the message) ends a mail as well.
        We interpret this as the end of mail, so functionally an empty mail.
     */
    fn test_dtp_first_line_transparency() {
        let input = ".\r\n.\r\n";
        let expected = ".\r\n";
        let result = DataState::decode_transparency(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_dtp_multi_line_transparency() {
        let input = ".abc\r\n.bcdef\r\ng\r\n.\r\n";
        let expected = "abc\r\nbcdef\r\ng\r\n.\r\n";
        let result = DataState::decode_transparency(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_dtp_double_period_transparency() {
        let input = "..a\r\n..bc\r\n.\r\n";
        let expected = ".a\r\n.bc\r\n.\r\n";
        let result = DataState::decode_transparency(input).unwrap();
        assert_eq!(result, expected);
    }

    //#[test]
    //Invalid test, dtp is not supposed to be called on str not containing a mail end
    fn test_dtp_multi_line_transparency_no_end() {
        let input = ".abc\r\n.bcdef\r\n";
        let expected = "abc\r\nbcdef\r\n";
        let result = DataState::decode_transparency(input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_dtp_rnp_end() {
        let input = "t\r\n.";
        let expected = "t\r\n.";
        let result = DataState::decode_transparency(input).unwrap();
        assert_eq!(result, expected);
    }
}
