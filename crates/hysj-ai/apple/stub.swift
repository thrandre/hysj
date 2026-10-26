// hysj-apple-ai, built without Apple's Foundation Models (an older SDK):
// sentence vectors still work (embed.swift); the model says it's unavailable,
// so Hysj hides the choice.
import Foundation

struct Request: Decodable {
    var embed: [String]?
}

func reply(_ value: [String: Any]) {
    let data = try! JSONSerialization.data(withJSONObject: value)
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write("\n".data(using: .utf8)!)
}

@main
struct Main {
    static func main() async {
        while let line = readLine() {
            if let texts = (try? JSONDecoder().decode(Request.self, from: Data(line.utf8)))?.embed {
                reply(await embed(texts))
            } else {
                reply(["available": false, "reason": "built without Foundation Models", "languages": [], "error": "unavailable"])
            }
        }
    }
}
