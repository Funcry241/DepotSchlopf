use super::*;

#[test]
fn http_liest_genau_eine_anfrage() -> Result<(), String> {
    let roh =
        b"POST /zeitplan?x=1 HTTP/1.1\r\nHost: 127.0.0.1:47410\r\nContent-Length: 3\r\n\r\na=b";
    let a = http::lies(&mut &roh[..]).map_err(|e| format!("{e:?}"))?;
    assert_eq!(a.methode, "POST");
    assert_eq!(a.pfad, "/zeitplan");
    assert_eq!(a.abfrage, "x=1");
    assert_eq!(a.kopf("host"), Some("127.0.0.1:47410"));
    assert_eq!(a.rumpf, b"a=b");
    Ok(())
}

#[test]
fn http_lehnt_unklare_anfragen_ab() {
    let faelle: [&[u8]; 6] = [
        b"GET / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n",
        b"POST / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 1\r\n\r\na",
        b"POST / HTTP/1.1\r\nContent-Length: 1\r\n\r\nab",
        b"GET http://fremd/ HTTP/1.1\r\n\r\n",
        b"GET / HTTP/2\r\n\r\n",
        b"GET / HTTP/1.1\r\nKaputt\r\n\r\n",
    ];
    for f in faelle {
        assert_eq!(http::lies(&mut &f[..]), Err(LeseFehler::Kaputt), "{f:?}");
    }
    let gross = format!(
        "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
        http::RUMPF_MAX + 1
    );
    assert_eq!(http::lies(&mut gross.as_bytes()), Err(LeseFehler::ZuGross));
    let mut riesig = b"GET / HTTP/1.1\r\nX: ".to_vec();
    riesig.extend(std::iter::repeat_n(b'a', http::KOPF_MAX + 10));
    assert_eq!(http::lies(&mut &riesig[..]), Err(LeseFehler::ZuGross));
    assert_eq!(
        http::lies(&mut &b"GET / HTTP/1.1\r\n"[..]),
        Err(LeseFehler::Abgebrochen)
    );
}

#[test]
fn fremder_host_wird_abgelehnt() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let umw = Attrappe::default();
    let t = donnerstag(12, 0)?;
    for host in [
        None,
        Some("boese.example:47410"),
        Some("127.0.0.1:80"),
        Some("127.0.0.1"),
        Some("localhost.boese.example:47410"),
    ] {
        let kopf: Vec<(&str, &str)> = host.map(|h| ("Host", h)).into_iter().collect();
        let a = anfrage("GET", "/", &kopf, "");
        assert_eq!(status(&behandle(&d, &umw, &a, t))?, 403, "{host:?}");
    }
    for host in [HOST, "localhost:47410"] {
        let a = anfrage("GET", "/", &[("Host", host)], "");
        assert_eq!(status(&behandle(&d, &umw, &a, t))?, 200, "{host}");
    }
    Ok(())
}

#[test]
fn post_braucht_herkunft_formular_und_token() -> Result<(), String> {
    let d = dienst(ordner()?, true);
    let umw = Attrappe::default();
    let t = donnerstag(12, 0)?;
    let rumpf = format!("t={TOKEN}&art=kurse");
    let ohne_herkunft = anfrage(
        "POST",
        "/ausfuehren",
        &[
            ("Host", HOST),
            ("Content-Type", "application/x-www-form-urlencoded"),
        ],
        &rumpf,
    );
    assert_eq!(status(&behandle(&d, &umw, &ohne_herkunft, t))?, 403);
    for herkunft in ["null", "http://boese.example", "http://127.0.0.1:47411"] {
        let a = anfrage(
            "POST",
            "/ausfuehren",
            &[
                ("Host", HOST),
                ("Origin", herkunft),
                ("Content-Type", "application/x-www-form-urlencoded"),
            ],
            &rumpf,
        );
        assert_eq!(status(&behandle(&d, &umw, &a, t))?, 403, "{herkunft}");
    }
    let json = anfrage(
        "POST",
        "/ausfuehren",
        &[
            ("Host", HOST),
            ("Origin", HERKUNFT),
            ("Content-Type", "text/plain"),
        ],
        &rumpf,
    );
    assert_eq!(status(&behandle(&d, &umw, &json, t))?, 400);
    for falsch in ["", "t=", "t=0123456789abcdef0123456789abcdee&art=kurse"] {
        let a = formular_post("/ausfuehren", falsch);
        let antwort = behandle(&d, &umw, &a, t);
        assert_eq!(status(&antwort)?, 403, "{falsch}");
        assert!(text(&antwort).contains("Zurück zu Live"));
    }
    assert!(wartend(&d).is_empty(), "nichts darf eingereiht sein");
    let a = formular_post("/ausfuehren", &rumpf);
    let r = behandle(&d, &umw, &a, t);
    assert_eq!(status(&r)?, 303);
    assert!(text(&r).contains("Location: /?m=auftrag\r\n"));
    assert_eq!(wartend(&d), vec![(Art::Kurse, Anlass::Hand)]);
    // Dieselbe Art nicht doppelt.
    let r = behandle(&d, &umw, &a, t);
    assert!(text(&r).contains("Location: /?m=auftrag_belegt\r\n"));
    assert_eq!(wartend(&d).len(), 1);
    Ok(())
}
