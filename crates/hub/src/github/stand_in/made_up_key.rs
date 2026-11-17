//! A private key made up for the tests, with `openssl genrsa -traditional 2048`, in the form GitHub gives an app's,
//! which no app on GitHub has, and which signs nothing anywhere but a test's stand-in.

/// The key of every app the stand-in makes, whose public half it checks what the hub signs as the app against.
pub(crate) const MADE_UP_APP_KEY: &str = "\
-----BEGIN RSA PRIVATE KEY-----\n\
MIIEowIBAAKCAQEAosiHvJwtMwqUxnGs8toxuMayz2c4LZYduKGR2Hwd4IGdUVmU\n\
r4U+fU6jtSDFmAHBjRgmIx4kz/XMER0+bTMvgncpgYD4QwFDyzH6ecMrzfVqN0CC\n\
88QilhXooZbBJi5s2QgFhN98Pk5VA7BF4iJ72wB7do79bZrjQMayE/kLQHLnZhP0\n\
uMLDEXHcb+6ZNue9jr8E4O30C6FBbTp3wemIc1ndDjDFzUXvCuccnta8GePm12rN\n\
5Yi08QMJBV50gEn5cxBZJI0Wf/pfolDgg+K7Akypfm+SzPv4mNlkGF65SqJx0n3H\n\
tuwSzt95W3d7CTT5qP0aWsnP5bOmfULY0KtyXwIDAQABAoIBAAxUUPJIXU9iNp4z\n\
NPdt7wSA9EErnxXWKr51v7Sxw0I3vwEkM6lPAnUo6X20vj1DAMnIMaHzogkoTle2\n\
KyjH5DTRrrtILUWcnmm7tC4tygRprq0BqLQq6mMLb25xxdkM/MyRbs2UxHXO/Axx\n\
TlG67T5LXTk/ylKCFUKuP1sDlXEWQ9nBOoTN8fg4m+qpyA1RjO/ZLVCOJWMvgMgW\n\
3wzY0FsKL9qtjuVlZn/EG57fmFG0S2JfUMjCw4EONKSwLnbwu5PWTrX8fs9ywDGU\n\
feBsEgUufdfXWMQqOd56/943VGcnkgkTmaAes2Obe3jAHk5+VoKvWgGxQZblroDp\n\
QDN1a2ECgYEA3x8IsC0Z91acsKzo6x0HqdwtUwONGaZLiu9J2bW9JZCGVW0kCGvm\n\
4JhF12Z6S4vrlLNG7uPEpmZmkTRgfeMqlw6bJl9Ye9GmaGAxj8bxVhK4nQ/xNNew\n\
ePBjTwUdkVBdxqpuRXmEEQNqpcfLsKsKvkeCC+nfQkMGNHmjTruAPSECgYEAusVT\n\
O4BZesKZVs+uhxkdFVGWtvnHqIiMM5PeJaOXGzLOJ4wA/paWh1XCv0TL2D5ydV2V\n\
CkOfRYTgMu/85fDXXJY7UoHJ8/uMAFFuEdT8wH2k7cnlpSVET6IFevske+erPwR1\n\
p+t5aZY47/+vp9EUVsg2dFJErghZMFBb/6rCP38CgYEAumWUvomPLcBfuoFZPEHO\n\
Syszeix3TjxM087AJW7ci2tHrko1Vgu+n1bW0fI8rKxBrMJwudXyTQkzGo9lJQYI\n\
aJW9K7tvklE88emavgB3UW7mZ+APpBOkzAQXEsa1Qtae4P80jDnOCOCP0dst4XOi\n\
z/+xNv60kwinQGN1+KpPocECgYAg5VZ2mXGpLJNDdgrgYqz7r0YjbjUbWwmOoBG1\n\
gW3A7Z7f2knmjXFv3swRUjWxHbadM2Fn0GdCIFg4wIpDmbrk5NUIADIo1fpfp/gC\n\
DgiCgt7EvxXCk6DHgdsDQS644CIKVGfArrnQvhPDOb6fiTTS4CqvLDpKiM/ksZKy\n\
oknaUwKBgHpye+UCgP1WuMtZlZWiF92cQwBEjES71v2PU4aZ6ijIRPEIVLPWkhHl\n\
H8Y7TuaDinywvWMqr0bUymXXtbDRhGWCn7nGNP5R5sEoHbbmd24OPvjxG+z/Qosc\n\
8KFrPZq3j5TJvRvYIhBAG+cWChH4T+mzMHT56Q0XQ7J48u4cfwAt\n\
-----END RSA PRIVATE KEY-----\n";
