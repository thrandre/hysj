// hysj-apple-ai, built without Apple's Foundation Models (an older SDK):
// says it's unavailable, so Hysj hides the choice.
import Foundation

@main
struct Main {
    static func main() {
        while let _ = readLine() {
            print(#"{"available": false, "reason": "built without Foundation Models", "languages": [], "error": "unavailable"}"#)
            fflush(stdout)
        }
    }
}
