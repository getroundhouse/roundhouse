// Xcode needs something to compile before it will sign an app. The project's last build phase puts the Rust
// binary in place of this one before signing, so this never runs.
@main
struct Stub {
    static func main() {}
}
