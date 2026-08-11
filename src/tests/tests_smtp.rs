#[cfg(test)]
mod tests_smtp {
    use tokio::net::TcpStream;
    use crate::smtp::smtp::{Smtp, StateKind};
    use crate::smtp::smtp_mail::MailAddress;
    use crate::tests::test::expect_msg;
    use crate::tests::test::test::{ehlo_msg, test_setup};

    #[tokio::test]
    async fn test_init() {
        let mut s: Smtp<TcpStream> = test_setup();
        s.init_smtp().await.unwrap();

        expect_msg!(s, "220 hi\r\n");
    }

    #[tokio::test]
    async fn test_n_unknown_cmd() {
        let mut s: Smtp<TcpStream> = test_setup();
        s.init_smtp().await.unwrap();
        expect_msg!(s, "220 hi\r\n");

        s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.handle("blub\r\n".to_string()).await.unwrap();
        expect_msg!(s, "500 Unrecognized command\r\n");
    }

    #[tokio::test]
    async fn test_helo_mail() {
        let sender = "sender@test.org";
        let rcpt = MailAddress::new("rcv@whalemail.net").unwrap();

        let mut s: Smtp<TcpStream> = test_setup();
        s.init_smtp().await.unwrap();
        expect_msg!(s, "220 hi\r\n");

        s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.handle("MAIL FROM:<".to_string() + sender + ">\r\n").await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.handle(format!("RCPT TO:<{}>\r\n", rcpt.address.as_str())).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.handle("DATA\r\n".to_string()).await.unwrap();
        expect_msg!(s, "354 start mail input\r\n");

        s.handle("<mailblob> blob blob\r\n.\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        let r = s.handle("QUIT\r\n".to_string()).await.unwrap();
        expect_msg!(s, "221 closing channel\r\n");
        assert_eq!(r, StateKind::QUIT);

        // Verify mail
        assert_eq!(s.mail.sender, Some(sender.to_string()));
        assert_eq!(s.mail.recipients, Vec::from([rcpt]));
        assert!(s.mail.is_finished());
    }

    #[tokio::test]
    async fn test_ehlo_mail() {
        let sender = "sender@test.org";
        let rcpt = MailAddress::new("rcv@whalemail.net").unwrap();

        let mut s: Smtp<TcpStream> = test_setup();
        s.init_smtp().await.unwrap();
        expect_msg!(s, "220 hi\r\n");

        s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, ehlo_msg(&s));

        s.handle("MAIL FROM:<".to_string() + sender + ">\r\n").await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.handle(format!("RCPT TO:<{}>\r\n", rcpt.address.as_str())).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.handle("DATA\r\n".to_string()).await.unwrap();
        expect_msg!(s, "354 start mail input\r\n");

        s.handle("<mailblob> blob blob\r\n.\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        let r = s.handle("QUIT\r\n".to_string()).await.unwrap();
        expect_msg!(s, "221 closing channel\r\n");
        assert_eq!(r, StateKind::QUIT);

        // Verify mail
        assert_eq!(s.mail.sender, Some(sender.to_string()));
        assert_eq!(s.mail.recipients, Vec::from([rcpt]));
        assert!(s.mail.is_finished());
    }

    #[tokio::test]
    async fn test_n_mail_parts() {
        let mailct_1 = "<mailblob> blob blob\r\n".to_string();
        let mailct_2 = "more blob\r\n.\r\n".to_string();
        let mut s: Smtp<TcpStream> = test_setup();
        s.init_smtp().await.unwrap();
        expect_msg!(s, "220 hi\r\n");

        s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.handle("MAIL FROM:<sender@test.org>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.handle("RCPT TO:<rcv@whalemail.net>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.handle("DATA\r\n".to_string()).await.unwrap();
        expect_msg!(s, "354 start mail input\r\n");

        s.handle(mailct_1.clone()).await.unwrap();
        assert!(!s.mail.is_finished());
        s.conn_writer.conn_testbed.as_mut().unwrap().expect_no_msg();

        s.handle(mailct_2.clone()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        let r = s.handle("QUIT\r\n".to_string()).await.unwrap();
        expect_msg!(s, "221 closing channel\r\n");
        assert_eq!(r, StateKind::QUIT);

        // verify msg
        assert_eq!(s.mail.body, mailct_1 + &mailct_2);
    }

    #[tokio::test]
    async fn test_helo_multiple_rcpt() {
        let rcpt1 = MailAddress::new("rcv@2whalemail.net").unwrap();
        let rcpt2 = MailAddress::new("rcv@2whalemail.net").unwrap();
        let mut s: Smtp<TcpStream> = test_setup();
        s.init_smtp().await.unwrap();
        expect_msg!(s, "220 hi\r\n");

        s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.handle("MAIL FROM:<sender@test.org>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.handle(format!("RCPT TO:<{}>\r\n", rcpt1.address.as_str())).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.handle(format!("RCPT TO:<{}>\r\n", rcpt2.address.as_str())).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        assert_eq!(s.mail.recipients, Vec::from([rcpt1, rcpt2]));
    }

    #[tokio::test]
    async fn test_n_omit_rcpt() {
        let mut s: Smtp<TcpStream> = test_setup();
        s.init_smtp().await.unwrap();
        expect_msg!(s, "220 hi\r\n");

        s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.handle("MAIL FROM:<sender@test.org>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.handle("DATA\r\n".to_string()).await.unwrap();
        expect_msg!(s, "503 Bad sequence\r\n");
    }

    /*
    Tests for decode_transparency
     */
    #[test]
    fn test_dtp_empty_s() {
        let mut smtp: Smtp<TcpStream> = test_setup();
        let expected = "".to_string();
        assert_eq!(smtp.decode_transparency(expected.clone()), false);
        assert_eq!(smtp.mail.body, expected);
    }

    #[test]
    fn test_dtp_simple_mail() {
        let mut smtp: Smtp<TcpStream> = test_setup();
        let expected = "blub\r\n.\r\n".to_string();
        assert_eq!(smtp.decode_transparency(expected.clone()), true);
        assert_eq!(smtp.mail.body, expected);
    }

    #[test]
    /*
        Interpretation of dot-stuffing. RFC says "\r\n.\r\n ends a mail", which
        does not specify whether ^.\r\n (with ^ beginning of the message) ends a mail as well.
        We interpret this as the end of mail, so functionally an empty mail.
     */
    fn test_dtp_first_line_transparency() {
        let mut smtp: Smtp<TcpStream> = test_setup();
        let input = ".\r\n.\r\n".to_string();
        let expected = ".\r\n".to_string();
        assert_eq!(smtp.decode_transparency(input), true);
        assert_eq!(smtp.mail.body, expected);
    }

    #[test]
    fn test_dtp_multi_line_transparency() {
        let mut smtp: Smtp<TcpStream> = test_setup();
        let input = ".abc\r\n.bcdef\r\ng\r\n.\r\n".to_string();
        let expected = "abc\r\nbcdef\r\ng\r\n.\r\n".to_string();
        assert_eq!(smtp.decode_transparency(input), true);
        assert_eq!(smtp.mail.body, expected);
    }

    #[test]
    fn test_dtp_double_period_transparency() {
        let mut smtp: Smtp<TcpStream> = test_setup();
        let input = "..a\r\n..bc\r\n.\r\n".to_string();
        let expected = ".a\r\n.bc\r\n.\r\n".to_string();
        assert_eq!(smtp.decode_transparency(input), true);
        assert_eq!(smtp.mail.body, expected);
    }

    #[test]
    fn test_dtp_multi_line_transparency_no_end() {
        let mut smtp: Smtp<TcpStream> = test_setup();
        let input = ".abc\r\n.bcdef\r\n".to_string();
        let expected = "abc\r\nbcdef\r\n".to_string();
        assert_eq!(smtp.decode_transparency(input), false);
        assert_eq!(smtp.mail.body, expected);
    }
}