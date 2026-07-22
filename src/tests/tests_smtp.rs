#[cfg(test)]
mod tests_smtp {
    use crate::smtp::smtp::StateKind;
    use crate::tests::test::expect_msg;
    use crate::tests::test::test::{test_setup, EHLO_MSG};

    #[tokio::test]
    async fn test_init() {
        let mut s = test_setup();
        s.init_smtp().await.unwrap();

        expect_msg!(s, "220 hi\r\n");
    }

    #[tokio::test]
    async fn test_n_unknown_cmd() {
        let mut s = test_setup();
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
        let rcpt = "rcv@whalemail.net";

        let mut s = test_setup();
        s.init_smtp().await.unwrap();
        expect_msg!(s, "220 hi\r\n");

        s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.handle("MAIL FROM:<".to_string() + sender + ">\r\n").await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.handle("RCPT TO:<".to_string() + rcpt + ">\r\n").await.unwrap();
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
        assert_eq!(s.mail.recipients, Vec::from([rcpt.to_string()]));
        assert!(s.mail.is_finished());
    }

    #[tokio::test]
    async fn test_ehlo_mail() {
        let sender = "sender@test.org";
        let rcpt = "rcv@whalemail.net";

        let mut s = test_setup();
        s.init_smtp().await.unwrap();
        expect_msg!(s, "220 hi\r\n");

        s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, EHLO_MSG);

        s.handle("MAIL FROM:<".to_string() + sender + ">\r\n").await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.handle("RCPT TO:<".to_string() + rcpt + ">\r\n").await.unwrap();
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
        assert_eq!(s.mail.recipients, Vec::from([rcpt.to_string()]));
        assert!(s.mail.is_finished());
    }

    #[tokio::test]
    async fn test_n_mail_parts() {
        let mailct_1 = "<mailblob> blob blob\r\n".to_string();
        let mailct_2 = "more blob\r\n.\r\n".to_string();
        let mut s = test_setup();
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
        s.conn_testbed.as_mut().unwrap().expect_no_msg();

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
        let rcpt1 = "rcv1@whalemail.net";
        let rcpt2 = "rcv2@whalemail.net";
        let mut s = test_setup();
        s.init_smtp().await.unwrap();
        expect_msg!(s, "220 hi\r\n");

        s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.handle("MAIL FROM:<sender@test.org>\r\n".to_string()).await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.handle("RCPT TO:<".to_string() + rcpt1 + ">\r\n").await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        s.handle("RCPT TO:<".to_string() + rcpt2 + ">\r\n").await.unwrap();
        expect_msg!(s, "250 OK\r\n");

        assert_eq!(s.mail.recipients, Vec::from([rcpt1.to_string(), rcpt2.to_string()]));
    }

    #[tokio::test]
    async fn test_n_omit_rcpt() {
        let mut s = test_setup();
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
        let mut smtp = test_setup();
        let expected = "".to_string();
        assert_eq!(smtp.decode_transparency(expected.clone()), false);
        assert_eq!(smtp.mail.body, expected);
    }

    #[test]
    fn test_dtp_simple_mail() {
        let mut smtp = test_setup();
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
        let mut smtp = test_setup();
        let input = ".\r\n.\r\n".to_string();
        let expected = ".\r\n".to_string();
        assert_eq!(smtp.decode_transparency(input), true);
        assert_eq!(smtp.mail.body, expected);
    }

    #[test]
    fn test_dtp_multi_line_transparency() {
        let mut smtp = test_setup();
        let input = ".abc\r\n.bcdef\r\ng\r\n.\r\n".to_string();
        let expected = "abc\r\nbcdef\r\ng\r\n.\r\n".to_string();
        assert_eq!(smtp.decode_transparency(input), true);
        assert_eq!(smtp.mail.body, expected);
    }

    #[test]
    fn test_dtp_double_period_transparency() {
        let mut smtp = test_setup();
        let input = "..a\r\n..bc\r\n.\r\n".to_string();
        let expected = ".a\r\n.bc\r\n.\r\n".to_string();
        assert_eq!(smtp.decode_transparency(input), true);
        assert_eq!(smtp.mail.body, expected);
    }

    #[test]
    fn test_dtp_multi_line_transparency_no_end() {
        let mut smtp = test_setup();
        let input = ".abc\r\n.bcdef\r\n".to_string();
        let expected = "abc\r\nbcdef\r\n".to_string();
        assert_eq!(smtp.decode_transparency(input), false);
        assert_eq!(smtp.mail.body, expected);
    }
}