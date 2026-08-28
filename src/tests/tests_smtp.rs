#[cfg(test)]
mod tests_smtp {
    use tokio::net::TcpStream;
    use crate::smtp::smtp::{Smtp2, StateKind};
    use crate::smtp::smtp_mail::MailAddress;
    use crate::tests::test::expect_msg;
    use crate::tests::test::test::{ehlo_msg, test_setup};

    #[tokio::test]
    async fn test_init() {
        let mut s: Smtp2<TcpStream> = test_setup().await;

        expect_msg!(s, "220 hi\r\n");
    }

    #[tokio::test]
    async fn test_n_unknown_cmd() {
        let mut s: Smtp2<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle("blub\r\n".to_string()).await.unwrap();
        expect_msg!(s, "500 Unrecognized command\r\n");
    }

    #[tokio::test]
    async fn test_helo_mail() {
        let sender = "sender@test.org";
        let rcpt = MailAddress::new("rcv@whalemail.net").unwrap();

        let mut s: Smtp2<TcpStream> = test_setup().await;
        s.user_db().lock().unwrap().mock_mailbox("rcv@whalemail.net".to_string());
        
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
        assert_eq!(s.mail().sender, sender.to_string());
        assert_eq!(s.mail().recipients, Vec::from([rcpt]));
        assert!(s.mail().is_finished());
    }

    #[tokio::test]
    async fn test_ehlo_mail() {
        let sender = "sender@test.org";
        let rcpt = MailAddress::new("rcv@whalemail.net").unwrap();

        let mut s: Smtp2<TcpStream> = test_setup().await;
        s.user_db().lock().unwrap().mock_mailbox("rcv@whalemail.net".to_string());
        
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
        assert_eq!(s.mail().sender, sender.to_string());
        assert_eq!(s.mail().recipients, Vec::from([rcpt]));
        assert!(s.mail().is_finished());
    }

    #[tokio::test]
    async fn test_n_mail_parts() {
        let mailct_1 = "<mailblob> blob blob\r\n".to_string();
        let mailct_2 = "more blob\r\n.\r\n".to_string();
        let mut s: Smtp2<TcpStream> = test_setup().await;
        s.user_db().lock().unwrap().mock_mailbox("rcv@whalemail.net".to_string());
        
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
        assert!(!s.mail().is_finished());
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
        let rcpt1 = MailAddress::new("rcv@2whalemail.net").unwrap();
        let rcpt2 = MailAddress::new("rcv@2whalemail.net").unwrap();
        let mut s: Smtp2<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle("MAIL FROM:<sender@test.org>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle(format!("RCPT TO:<{}>\r\n", rcpt1.address.as_str())).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle(format!("RCPT TO:<{}>\r\n", rcpt2.address.as_str())).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        assert_eq!(s.mail().recipients, Vec::from([rcpt1, rcpt2]));
    }

    #[tokio::test]
    async fn test_n_omit_rcpt() {
        let mut s: Smtp2<TcpStream> = test_setup().await;
        expect_msg!(s, "220 hi\r\n");

        (s, _) = s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle("MAIL FROM:<sender@test.org>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        (s, _) = s.handle("DATA\r\n".to_string()).await.unwrap();
        expect_msg!(s, "503 Bad sequence\r\n");
    }

    /*
    Tests for decode_transparency
     */
    #[tokio::test]
    async fn test_dtp_empty_s() {
        let s: Smtp2<TcpStream> = test_setup().await;
        let expected = "".to_string();

        let (result, mail_body) = s.decode_transparency(expected.clone());
        assert_eq!(result, false);
        assert_eq!(mail_body, expected);
    }

    #[tokio::test]
    async fn test_dtp_simple_mail() {
        let mut s: Smtp2<TcpStream> = test_setup().await;
        let expected = "blub\r\n.\r\n".to_string();
        
        let (result, mail_body) = s.decode_transparency(expected.clone());
        assert_eq!(result, true);
        assert_eq!(mail_body, expected);
    }

    #[tokio::test]
    /*
        Interpretation of dot-stuffing. RFC says "\r\n.\r\n ends a mail", which
        does not specify whether ^.\r\n (with ^ beginning of the message) ends a mail as well.
        We interpret this as the end of mail, so functionally an empty mail.
     */
    async fn test_dtp_first_line_transparency() {
        let s: Smtp2<TcpStream> = test_setup().await;
        let input = ".\r\n.\r\n".to_string();
        let expected = ".\r\n".to_string();
        let (result, mail_body) = s.decode_transparency(input);
        assert_eq!(result, true);
        assert_eq!(mail_body, expected);
    }

    #[tokio::test]
    async fn test_dtp_multi_line_transparency() {
        let s: Smtp2<TcpStream> = test_setup().await;
        let input = ".abc\r\n.bcdef\r\ng\r\n.\r\n".to_string();
        let expected = "abc\r\nbcdef\r\ng\r\n.\r\n".to_string();
        let (result, mail_body) = s.decode_transparency(input);
        assert_eq!(result, true);
        assert_eq!(mail_body, expected);
    }

    #[tokio::test]
    async fn test_dtp_double_period_transparency() {
        let s: Smtp2<TcpStream> = test_setup().await;
        let input = "..a\r\n..bc\r\n.\r\n".to_string();
        let expected = ".a\r\n.bc\r\n.\r\n".to_string();
        let (result, mail_body) = s.decode_transparency(input);
        assert_eq!(result, true);
        assert_eq!(mail_body, expected);
    }

    #[tokio::test]
    async fn test_dtp_multi_line_transparency_no_end() {
        let s: Smtp2<TcpStream> = test_setup().await;
        let input = ".abc\r\n.bcdef\r\n".to_string();
        let expected = "abc\r\nbcdef\r\n".to_string();
        let (result, mail_body) = s.decode_transparency(input);
        assert_eq!(result, false);
        assert_eq!(mail_body, expected);
    }
}