//! The Expect session against a fake device on an in-memory duplex.
//!
//! Expected matches and buffers are the prototype's (`Expect` module of
//! `docs/prototypes/expect-api.PROTOTYPE.html`), run on the same scenario
//! steps.

use std::time::Duration;

use justelnet::core::{self, OptionPolicy, Side, TelnetOption};
use justelnet::{Client, Event};
use justelnet_expect::regex::bytes::Regex;
use justelnet_expect::{Error, Expect, Logged, Match, Pattern};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream, duplex};

const SECS_5: Duration = Duration::from_secs(5);

/// An Expect session on a client whose policy sends nothing unasked.
fn session() -> (Expect<DuplexStream>, DuplexStream) {
    let (stream, device) = duplex(1024);
    let client = Client::new(stream, OptionPolicy::builder().refuse_all().build());
    (Expect::new(client), device)
}

/// Every byte the device can read without waiting.
async fn received(device: &mut DuplexStream) -> Vec<u8> {
    let mut received = Vec::new();
    let mut buf = [0; 1024];
    while let Ok(Ok(n)) =
        tokio::time::timeout(Duration::from_millis(10), device.read(&mut buf)).await
    {
        if n == 0 {
            break;
        }
        received.extend_from_slice(&buf[..n]);
    }
    received
}

fn assert_match(m: &Match, index: usize, before: &[u8], matched: &[u8]) {
    assert_eq!(m.index, index);
    assert_eq!(m.before, before, "before");
    assert_eq!(m.matched, matched, "matched");
}

/// Scenario 1, a normal login.
#[tokio::test(start_paused = true)]
async fn a_normal_login() {
    let (mut s, mut device) = session();

    device
        .write_all(b"\r\nUser Access Verification\r\n\r\nUsername: ")
        .await
        .unwrap();
    let m = s.expect("Username: ", SECS_5).await.unwrap();
    assert_match(
        &m,
        0,
        b"\r\nUser Access Verification\r\n\r\n",
        b"Username: ",
    );

    s.send_line("admin").await.unwrap();
    assert_eq!(received(&mut device).await, b"admin\r\n");
    device.write_all(b"admin\r\nPassword: ").await.unwrap();
    let m = s.expect("Password: ", SECS_5).await.unwrap();
    assert_match(&m, 0, b"admin\r\n", b"Password: ");

    s.send_line("********").await.unwrap();
    assert_eq!(received(&mut device).await, b"********\r\n");
    device.write_all(b"\r\nR1>").await.unwrap();
    let m = s.expect_any(&[">", "#"], SECS_5).await.unwrap();
    assert_match(&m, 0, b"\r\nR1", b">");
}

/// Scenario 3: the failure message wins, and the `Username: ` after it is
/// matched by the next wait without reading.
#[tokio::test(start_paused = true)]
async fn a_wrong_password_branches_and_keeps_the_next_prompt() {
    let (mut s, mut device) = session();
    device
        .write_all(b"\r\n% Login invalid\r\n\r\nUsername: ")
        .await
        .unwrap();

    let m = s
        .expect_any(&["#", ">", "% Login invalid"], SECS_5)
        .await
        .unwrap();
    assert_match(&m, 2, b"\r\n", b"% Login invalid");

    let start = tokio::time::Instant::now();
    let m = s.expect("Username: ", SECS_5).await.unwrap();
    assert_match(&m, 0, b"\r\n\r\n", b"Username: ");
    assert_eq!(start.elapsed(), Duration::ZERO);
}

/// Scenario 4: a `--More--` loop collects every page.
#[tokio::test(start_paused = true)]
async fn a_more_loop_collects_the_full_output() {
    let (mut s, mut device) = session();
    let pages: [&[u8]; 2] = [
        b"show run\r\nhostname R1\r\ninterface Gi0/0\r\n ip address 10.0.0.1 255.255.255.0\r\n --More-- ",
        b"\r\ninterface Gi0/1\r\n shutdown\r\nend\r\n\r\nR1#",
    ];
    device.write_all(pages[0]).await.unwrap();

    let mut output = Vec::new();
    let mut waits = Vec::new();
    loop {
        assert!(waits.len() < pages.len(), "the loop did not end: {waits:?}");
        let m = s.expect_any(&["--More--", "R1#"], SECS_5).await.unwrap();
        output.extend_from_slice(&m.before);
        waits.push(m.index);
        if m.index == 1 {
            break;
        }
        s.send_raw(b" ").await.unwrap();
        assert_eq!(received(&mut device).await, b" ");
        device.write_all(pages[1]).await.unwrap();
    }

    assert_eq!(waits, [0, 1]);
    assert_eq!(
        output,
        b"show run\r\nhostname R1\r\ninterface Gi0/0\r\n ip address 10.0.0.1 255.255.255.0\r\n  \r\ninterface Gi0/1\r\n shutdown\r\nend\r\n\r\n"
    );
}

#[tokio::test(start_paused = true)]
async fn the_earliest_match_wins_whatever_its_place_in_the_list() {
    let (mut s, mut device) = session();
    device.write_all(b"\r\nR1>show\r\nR1#").await.unwrap();

    let m = s.expect_any(&["#", ">"], SECS_5).await.unwrap();

    assert_match(&m, 1, b"\r\nR1", b">");
    assert_eq!(s.into_inner().1, b"show\r\nR1#");
}

#[tokio::test(start_paused = true)]
async fn at_the_same_position_the_pattern_listed_first_wins() {
    for (patterns, index, matched, rest) in [
        (["R", "R1"], 0, &b"R"[..], &b"1>"[..]),
        (["R1", "R"], 0, b"R1", b">"),
    ] {
        let (mut s, mut device) = session();
        device.write_all(b"R1>").await.unwrap();

        let m = s.expect_any(&patterns, SECS_5).await.unwrap();

        assert_match(&m, index, b"", matched);
        assert_eq!(s.into_inner().1, rest, "{patterns:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn literals_and_regexes_mix_in_one_wait() {
    let (mut s, mut device) = session();
    device
        .write_all(b"\r\n% Login invalid\r\n\r\nUsername: ")
        .await
        .unwrap();
    let prompt = prompt();

    let m = s
        .expect_any(
            &[Pattern::from(&prompt), Pattern::from("Username: ")],
            SECS_5,
        )
        .await
        .unwrap();

    assert_match(&m, 1, b"\r\n% Login invalid\r\n\r\n", b"Username: ");
}

#[tokio::test(start_paused = true)]
async fn no_patterns_wait_for_the_timeout() {
    let (mut s, mut device) = session();
    device.write_all(b"R1>").await.unwrap();

    let result = s.expect_any::<&str>(&[], SECS_5).await;

    assert!(
        matches!(result, Err(Error::Timeout { ref buffer }) if buffer == b"R1>"),
        "{result:?}"
    );
}

/// Scenario 2: a prompt split across reads still matches, and not before
/// its second half arrives.
#[tokio::test(start_paused = true)]
async fn a_prompt_split_across_reads() {
    let (mut s, mut device) = session();
    device.write_all(b"Pass").await.unwrap();
    let start = tokio::time::Instant::now();

    let (m, ()) = tokio::join!(s.expect("Password: ", SECS_5), async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        device.write_all(b"word: ").await.unwrap();
    });

    let m = m.unwrap();
    assert_match(&m, 0, b"", b"Password: ");
    assert_eq!(start.elapsed(), Duration::from_secs(1));
}

/// Scenario 5: a timeout carries what had arrived, and keeps it buffered.
#[tokio::test(start_paused = true)]
async fn a_timeout_carries_the_confirm_buffer() {
    let (mut s, mut device) = session();
    s.send_line("reload").await.unwrap();
    device
        .write_all(b"reload\r\nProceed with reload? [confirm]")
        .await
        .unwrap();
    let start = tokio::time::Instant::now();

    let result = s.expect("#", SECS_5).await;

    assert_eq!(start.elapsed(), SECS_5);
    match result {
        Err(Error::Timeout { buffer }) => {
            assert_eq!(buffer, b"reload\r\nProceed with reload? [confirm]")
        }
        other => panic!("expected a timeout, got {other:?}"),
    }
    let m = s.expect("[confirm]", SECS_5).await.unwrap();
    assert_match(&m, 0, b"reload\r\nProceed with reload? ", b"[confirm]");
}

const BANNER: &[u8] = b"\r\n*** Authorized access only. Ticket #4521 ***\r\nR1#";

/// Scenario 6, first half: a literal `#` matches inside the banner.
#[tokio::test(start_paused = true)]
async fn a_literal_hash_matches_inside_the_banner() {
    let (mut s, mut device) = session();
    device.write_all(BANNER).await.unwrap();

    let m = s.expect("#", SECS_5).await.unwrap();

    assert_match(&m, 0, b"\r\n*** Authorized access only. Ticket ", b"#");
    assert_eq!(s.into_inner().1, b"4521 ***\r\nR1#");
}

/// Scenario 6, second half: an anchored regex matches only the real prompt.
#[tokio::test(start_paused = true)]
async fn an_anchored_regex_matches_only_the_real_prompt() {
    let (mut s, mut device) = session();
    device.write_all(BANNER).await.unwrap();

    let m = s.expect(prompt(), SECS_5).await.unwrap();

    assert_match(
        &m,
        0,
        b"\r\n*** Authorized access only. Ticket #4521 ***\r",
        b"\nR1#",
    );
    assert_eq!(s.into_inner().1, b"");
}

/// A prompt whose hostname is EUC-KR ("라우터") needs `(?-u:\S)`.
#[tokio::test(start_paused = true)]
async fn a_non_utf8_prompt_needs_a_byte_class() {
    let (mut s, mut device) = session();
    device
        .write_all(b"\r\n\xb6\xf3\xbf\xec\xc5\xcd#")
        .await
        .unwrap();

    let unicode = Regex::new(r"\n\S+[#>] ?$").unwrap();
    assert!(matches!(
        s.expect(&unicode, Duration::ZERO).await,
        Err(Error::Timeout { .. })
    ));
    let bytes = Regex::new(r"\n(?-u:\S)+[#>] ?$").unwrap();
    let m = s.expect(&bytes, SECS_5).await.unwrap();

    assert_match(&m, 0, b"\r", b"\n\xb6\xf3\xbf\xec\xc5\xcd#");
}

#[tokio::test(start_paused = true)]
async fn a_borrowed_regex_is_a_pattern() {
    let (mut s, mut device) = session();
    let prompt = prompt();
    device.write_all(b"\r\nR1#show\r\nR1#").await.unwrap();

    let m = s.expect(&prompt, SECS_5).await.unwrap();

    assert_match(&m, 0, b"\r\nR1#show\r", b"\nR1#");
}

/// The prompt regex of scenario 6.
fn prompt() -> Regex {
    Regex::new(r"\n\S+[#>] ?$").unwrap()
}

/// "허가된 사용자만 접속하십시오" in EUC-KR, which is not UTF-8.
const EUC_KR_BANNER: &[u8] = b"\xc7\xe3\xb0\xa1\xb5\xc8 \xbb\xe7\xbf\xeb\xc0\xda\xb8\xb8 \xc1\xa2\xbc\xd3\xc7\xcf\xbd\xca\xbd\xc3\xbf\xc0";

#[tokio::test(start_paused = true)]
async fn an_ascii_prompt_matches_after_an_euc_kr_banner() {
    let (mut s, mut device) = session();
    let mut banner = b"\r\n".to_vec();
    banner.extend_from_slice(EUC_KR_BANNER);
    banner.extend_from_slice(b"\r\n");
    device.write_all(&banner).await.unwrap();
    device.write_all(b"Username: \r\n...\r\nR1>").await.unwrap();

    let m = s.expect("Username: ", SECS_5).await.unwrap();
    assert_match(&m, 0, &banner, b"Username: ");

    let m = s.expect(prompt(), SECS_5).await.unwrap();
    assert_match(&m, 0, b"\r\n...\r", b"\nR1>");
}

#[tokio::test(start_paused = true)]
async fn an_anchored_regex_matches_across_an_euc_kr_banner() {
    let (mut s, mut device) = session();
    let mut banner = b"\r\n".to_vec();
    banner.extend_from_slice(EUC_KR_BANNER);
    banner.extend_from_slice(b"\r");
    device.write_all(&banner).await.unwrap();
    device.write_all(b"\nR1>").await.unwrap();

    let m = s.expect(prompt(), SECS_5).await.unwrap();

    assert_match(&m, 0, &banner, b"\nR1>");
}

#[tokio::test(start_paused = true)]
async fn lossy_views_show_before_and_matched_as_text() {
    let (mut s, mut device) = session();
    device.write_all(EUC_KR_BANNER).await.unwrap();
    device.write_all(b"\r\nR1#").await.unwrap();

    let m = s.expect(prompt(), SECS_5).await.unwrap();

    let before = m.before_lossy();
    assert!(before.contains('\u{FFFD}'), "{before:?}");
    assert!(before.ends_with("\u{FFFD}\r"), "{before:?}");
    assert_eq!(m.matched_lossy(), "\nR1#");
}

#[tokio::test(start_paused = true)]
async fn data_after_the_match_stays_buffered() {
    let (mut s, mut device) = session();
    device
        .write_all(b"\r\n% Login invalid\r\n\r\nUsername: ")
        .await
        .unwrap();

    let m = s.expect("% Login invalid", SECS_5).await.unwrap();
    assert_match(&m, 0, b"\r\n", b"% Login invalid");
    drop(device);

    let m = s.expect("Username: ", SECS_5).await.unwrap();
    assert_match(&m, 0, b"\r\n\r\n", b"Username: ");
}

#[tokio::test(start_paused = true)]
async fn non_data_events_are_skipped() {
    let (mut s, mut device) = session();
    device
        .write_all(b"lo\xff\xfb\x01gi\xff\xf1n: ")
        .await
        .unwrap();

    let m = s.expect("login: ", SECS_5).await.unwrap();

    assert_match(&m, 0, b"", b"login: ");
}

#[tokio::test(start_paused = true)]
async fn a_close_in_the_middle_of_a_wait_carries_what_arrived() {
    let (mut s, mut device) = session();
    let start = tokio::time::Instant::now();

    let (result, ()) = tokio::join!(s.expect("#", SECS_5), async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        device.write_all(b"\r\n%SYS-5-RELOAD: ").await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        drop(device);
    });

    assert_eq!(start.elapsed(), Duration::from_secs(2));
    match result {
        Err(Error::Closed { buffer }) => assert_eq!(buffer, b"\r\n%SYS-5-RELOAD: "),
        other => panic!("expected a close, got {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn past_the_limit_the_oldest_bytes_go_and_a_prompt_at_the_tail_matches() {
    let (mut s, mut device) = session();
    s.set_buffer_limit(16);
    for line in [&b"0123456789\r\n"[..], b"abcdefghij\r\n", b"ABCDEFGHIJ\r\n"] {
        device.write_all(line).await.unwrap();
    }
    device.write_all(b"R1#").await.unwrap();

    let m = s.expect("R1#", SECS_5).await.unwrap();

    assert_match(&m, 0, b"\nABCDEFGHIJ\r\n", b"R1#");
}

#[tokio::test(start_paused = true)]
async fn by_default_a_wait_keeps_the_last_mebibyte() {
    let (mut s, mut device) = session();
    let noise: Vec<u8> = (0..(1 << 20) + 100)
        .map(|i| b'a' + (i % 26) as u8)
        .collect();

    let never = Regex::new(r"#$").unwrap();

    let (result, ()) = tokio::join!(s.expect(&never, SECS_5), async {
        device.write_all(&noise).await.unwrap();
    });

    match result {
        Err(Error::Timeout { buffer }) => {
            assert_eq!(buffer.len(), 1 << 20);
            assert_eq!(buffer, noise[100..]);
        }
        other => panic!("expected a timeout, got {:?}", other.map(|m| m.index)),
    }
    assert_eq!(justelnet_expect::DEFAULT_BUFFER_LIMIT, 1 << 20);
}

/// Writes each chunk as its own read, a millisecond apart.
async fn write_reads(device: &mut DuplexStream, chunks: &[&[u8]]) {
    for chunk in chunks {
        device.write_all(chunk).await.unwrap();
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

#[tokio::test(start_paused = true)]
async fn a_literal_split_at_any_point_across_reads_matches() {
    let prompt = b"Password: ";
    for split in 1..prompt.len() {
        let (mut s, mut device) = session();
        let first = [&b"\r\nUser OK\r\n"[..], &prompt[..split]].concat();
        let reads = [&first[..], &prompt[split..]];

        let (m, ()) = tokio::join!(
            s.expect("Password: ", SECS_5),
            write_reads(&mut device, &reads)
        );

        assert_match(&m.unwrap(), 0, b"\r\nUser OK\r\n", b"Password: ");
    }
}

#[tokio::test(start_paused = true)]
async fn a_literal_split_across_reads_matches_after_the_limit_drops_bytes() {
    let (mut s, mut device) = session();
    s.set_buffer_limit(16);

    let (m, ()) = tokio::join!(
        s.expect("R1#", SECS_5),
        write_reads(&mut device, &[b"0123456789\r\n", b"abcdefghij\r\nR1", b"#"])
    );

    assert_match(&m.unwrap(), 0, b"\nabcdefghij\r\n", b"R1#");
}

#[tokio::test(start_paused = true)]
async fn a_literal_split_across_reads_still_wins_over_a_later_regex() {
    let (mut s, mut device) = session();
    let error = Regex::new(r"% \w+").unwrap();
    let patterns = [Pattern::from(&error), Pattern::from("Username: ")];

    let (m, ()) = tokio::join!(
        s.expect_any(&patterns, SECS_5),
        write_reads(&mut device, &[b"\r\nUser", b"name: % Bad"])
    );

    assert_match(&m.unwrap(), 1, b"\r\n", b"Username: ");
    assert_eq!(s.into_inner().1, b"% Bad");
}

#[tokio::test(start_paused = true)]
async fn a_literal_split_across_reads_ties_with_a_regex_by_list_order() {
    let r1 = Regex::new("R1").unwrap();
    for (patterns, index, matched, rest) in [
        (
            [Pattern::from(&r1), Pattern::from("R1>")],
            0,
            &b"R1"[..],
            &b">"[..],
        ),
        ([Pattern::from("R1>"), Pattern::from(&r1)], 0, b"R1>", b""),
    ] {
        let (mut s, mut device) = session();

        let (m, ()) = tokio::join!(
            s.expect_any(&patterns, SECS_5),
            write_reads(&mut device, &[b"\r\nR", b"1>"])
        );

        assert_match(&m.unwrap(), index, b"\r\n", matched);
        assert_eq!(s.into_inner().1, rest);
    }
}

#[tokio::test(start_paused = true)]
async fn a_session_shows_only_the_bytes_it_keeps() {
    let (mut s, mut device) = session();
    s.set_buffer_limit(16);
    device.write_all(b"0123456789abcdefghij").await.unwrap();
    let result = s.expect("#", SECS_5).await;
    assert!(matches!(result, Err(Error::Timeout { .. })), "{result:?}");

    let shown = format!("{s:?}");

    assert!(
        shown.contains(&format!("buffer: {:?}, ", b"456789abcdefghij")),
        "{shown}"
    );
    assert!(!shown.contains("head"), "{shown}");
}

#[tokio::test(start_paused = true)]
async fn the_next_wait_searches_what_the_last_one_left_from_its_start() {
    let (mut s, mut device) = session();
    let (result, ()) = tokio::join!(
        s.expect("#", SECS_5),
        write_reads(&mut device, &[b"R1>show", b" version"])
    );
    assert!(matches!(result, Err(Error::Timeout { .. })), "{result:?}");

    let m = s.expect(">", SECS_5).await.unwrap();

    assert_match(&m, 0, b"R1", b">");
}

#[tokio::test(start_paused = true)]
async fn after_many_reads_past_the_limit_only_the_newest_bytes_are_seen() {
    let (mut s, mut device) = session();
    s.set_buffer_limit(8);
    let lines: Vec<Vec<u8>> = (10..40)
        .map(|i| format!("line {i}\n").into_bytes())
        .collect();
    let lines: Vec<&[u8]> = lines.iter().map(Vec::as_slice).collect();

    let (result, ()) = tokio::join!(s.expect("#", SECS_5), write_reads(&mut device, &lines));
    match result {
        Err(Error::Timeout { buffer }) => assert_eq!(buffer, b"line 39\n"),
        other => panic!("expected a timeout, got {other:?}"),
    }

    device.write_all(b"R1>").await.unwrap();
    let m = s.expect("R1>", SECS_5).await.unwrap();

    assert_match(&m, 0, b"e 39\n", b"R1>");
    assert_eq!(s.into_inner().1, b"");
}

#[tokio::test(start_paused = true)]
async fn the_buffer_handed_back_holds_only_the_newest_bytes() {
    let (mut s, mut device) = session();
    s.set_buffer_limit(16);
    device.write_all(b"0123456789abcdefghij").await.unwrap();
    let result = s.expect("#", SECS_5).await;
    assert!(matches!(result, Err(Error::Timeout { .. })), "{result:?}");

    assert_eq!(s.into_inner().1, b"456789abcdefghij");
}

#[tokio::test(start_paused = true)]
async fn lowering_the_limit_trims_what_is_buffered() {
    let (mut s, mut device) = session();
    device.write_all(b"R1>show version\r\n").await.unwrap();
    s.expect(">", SECS_5).await.unwrap();

    s.set_buffer_limit(4);

    assert_eq!(s.into_inner().1, b"on\r\n");
}

#[tokio::test(start_paused = true)]
async fn a_close_during_a_wait_carries_the_buffer() {
    let (mut s, mut device) = session();
    device.write_all(b"Connection closed by ").await.unwrap();
    drop(device);

    match s.expect("#", SECS_5).await {
        Err(Error::Closed { buffer }) => assert_eq!(buffer, b"Connection closed by "),
        other => panic!("expected a close, got {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn the_client_comes_back_working_with_the_rest_of_the_buffer() {
    let (mut s, mut device) = session();
    device.write_all(b"\r\nR1>show ver").await.unwrap();
    s.expect(">", SECS_5).await.unwrap();
    s.send_command(core::Command::AreYouThere).await.unwrap();
    s.send_raw(b"\xff").await.unwrap();
    assert_eq!(received(&mut device).await, [0xff, 0xf6, 0xff, 0xff]);

    let (mut client, rest) = s.into_inner();

    assert_eq!(rest, b"show ver");
    assert!(!client.is_enabled(TelnetOption::ECHO, Side::Remote));
    client.send_data(b"x").await.unwrap();
    assert_eq!(received(&mut device).await, b"x");
    device.write_all(b"ion").await.unwrap();
    assert_eq!(
        client.next_event().await.unwrap(),
        Event::Core(core::Event::Data(b"ion".to_vec()))
    );
}

#[test]
fn errors_describe_themselves() {
    let timeout = Error::Timeout {
        buffer: b"[confirm]".to_vec(),
    };
    assert_eq!(
        timeout.to_string(),
        "timed out waiting for a pattern (9 bytes buffered)"
    );
    let closed = Error::Closed { buffer: Vec::new() };
    assert_eq!(
        closed.to_string(),
        "the connection closed while waiting for a pattern (0 bytes buffered)"
    );
    let client = Error::from(justelnet::Error::Closed);
    assert_eq!(client.to_string(), "the connection is closed");
}

#[tokio::test(start_paused = true)]
async fn a_timeout_too_large_to_add_still_matches_what_arrived() {
    let (mut s, mut device) = session();
    device.write_all(b"R1#").await.unwrap();

    let m = s.expect("#", Duration::MAX).await.unwrap();

    assert_match(&m, 0, b"R1", b"#");
}

#[tokio::test(start_paused = true)]
async fn a_timeout_too_large_to_add_waits_until_the_close() {
    let (mut s, mut device) = session();
    let start = tokio::time::Instant::now();
    let a_year = Duration::from_secs(365 * 24 * 60 * 60);

    let ((result, waited), ()) = tokio::join!(
        async { (s.expect("#", Duration::MAX).await, start.elapsed()) },
        async {
            tokio::time::sleep(a_year).await;
            device.write_all(b"R1>").await.unwrap();
            drop(device);
        }
    );

    assert!(
        matches!(result, Err(Error::Closed { ref buffer }) if buffer == b"R1>"),
        "{result:?}"
    );
    assert_eq!(waited, a_year);
}

#[tokio::test(start_paused = true)]
async fn the_timeout_counts_from_the_start_of_the_wait() {
    let (mut s, mut device) = session();
    let start = tokio::time::Instant::now();

    let ((result, waited), ()) = tokio::join!(
        async { (s.expect("#", SECS_5).await, start.elapsed()) },
        async {
            for _ in 0..5 {
                tokio::time::sleep(Duration::from_secs(2)).await;
                device.write_all(b".").await.unwrap();
            }
        }
    );

    assert!(matches!(result, Err(Error::Timeout { .. })), "{result:?}");
    assert_eq!(waited, SECS_5);
    assert!(matches!(
        s.expect("#", Duration::ZERO).await,
        Err(Error::Timeout { buffer }) if buffer == b"....."
    ));
}

#[tokio::test(start_paused = true)]
async fn the_first_occurrence_matches() {
    let (mut s, mut device) = session();
    device.write_all(b"R1>R2>").await.unwrap();

    let m = s.expect(">", SECS_5).await.unwrap();
    assert_match(&m, 0, b"R1", b">");
    let m = s.expect("", SECS_5).await.unwrap();
    assert_match(&m, 0, b"", b"");
    let m = s.expect(">", SECS_5).await.unwrap();
    assert_match(&m, 0, b"R2", b">");
}

#[tokio::test(start_paused = true)]
async fn option_state_is_read_through_the_session() {
    let (stream, mut device) = duplex(1024);
    let policy = OptionPolicy::builder()
        .refuse_all()
        .accept(TelnetOption::ECHO, Side::Remote)
        .build();
    let mut s = Expect::new(Client::new(stream, policy));
    assert!(!s.client().is_enabled(TelnetOption::ECHO, Side::Remote));
    device.write_all(b"\xff\xfb\x01login: ").await.unwrap();

    s.expect("login: ", SECS_5).await.unwrap();

    assert!(s.client().is_enabled(TelnetOption::ECHO, Side::Remote));
}

/// What a session's log was given, owned, in order.
#[derive(Debug, PartialEq)]
enum Entry {
    Sent(Vec<u8>),
    SentSecret,
    Received(Vec<u8>),
}

/// Sets a log on `s` that collects its entries.
fn log_into(s: &mut Expect<DuplexStream>) -> std::sync::Arc<std::sync::Mutex<Vec<Entry>>> {
    let entries = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = entries.clone();
    s.set_log(move |logged| {
        let entry = match logged {
            Logged::Sent(bytes) => Entry::Sent(bytes.to_vec()),
            Logged::SentSecret => Entry::SentSecret,
            Logged::Received(bytes) => Entry::Received(bytes.to_vec()),
            other => panic!("unexpected {other:?}"),
        };
        sink.lock().unwrap().push(entry);
    });
    entries
}

#[tokio::test(start_paused = true)]
async fn a_logged_login_records_both_directions_but_not_the_secret() {
    let (mut s, mut device) = session();
    let log = log_into(&mut s);

    device.write_all(b"Username: ").await.unwrap();
    s.expect("Username: ", SECS_5).await.unwrap();
    s.send_line("admin").await.unwrap();
    device.write_all(b"Password: ").await.unwrap();
    s.expect("Password: ", SECS_5).await.unwrap();
    s.send_secret_line("s3cret").await.unwrap();
    device.write_all(b"R1#").await.unwrap();
    s.expect("#", SECS_5).await.unwrap();

    assert_eq!(
        *log.lock().unwrap(),
        [
            Entry::Received(b"Username: ".to_vec()),
            Entry::Sent(b"admin\r".to_vec()),
            Entry::Received(b"Password: ".to_vec()),
            Entry::SentSecret,
            Entry::Received(b"R1#".to_vec()),
        ]
    );
    assert_eq!(received(&mut device).await, b"admin\r\ns3cret\r\n");
}

#[tokio::test(start_paused = true)]
async fn raw_bytes_are_logged_and_telnet_commands_are_not() {
    let (mut s, mut device) = session();
    let log = log_into(&mut s);

    s.send_raw(b"\x03").await.unwrap();
    s.send_command(core::Command::AreYouThere).await.unwrap();

    assert_eq!(*log.lock().unwrap(), [Entry::Sent(b"\x03".to_vec())]);
    assert_eq!(received(&mut device).await, [0x03, 0xff, 0xf6]);
}

#[tokio::test(start_paused = true)]
async fn each_read_of_a_wait_is_logged_as_it_arrives() {
    let (mut s, mut device) = session();
    let log = log_into(&mut s);

    let (m, ()) = tokio::join!(
        s.expect("#", SECS_5),
        write_reads(&mut device, &[b"show ", b"ver\r\nR1", b"#"])
    );
    m.unwrap();

    assert_eq!(
        *log.lock().unwrap(),
        [
            Entry::Received(b"show ".to_vec()),
            Entry::Received(b"ver\r\nR1".to_vec()),
            Entry::Received(b"#".to_vec()),
        ]
    );
}

#[test]
fn a_session_stays_send_and_sync_with_a_log() {
    fn send_and_sync<T: Send + Sync>() {}
    send_and_sync::<Expect<DuplexStream>>();
}
