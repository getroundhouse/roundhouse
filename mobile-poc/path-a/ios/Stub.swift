// Xcode needs something to compile before it will sign an app. scripts/device.sh swaps this
// executable for the Rust binary before signing, so this code never runs.
@main
struct Stub {
    static func main() {}
}
