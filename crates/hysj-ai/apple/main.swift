// hysj-apple-ai: Apple's on-device model (Foundation Models) for Hysj.
// One JSON request per line on stdin, one JSON answer per line on stdout:
//   {"check": true}                      → {"available": true, "reason": "", "languages": ["en", "nb", …]}
//   {"system", "user", "schema", "temperature", "max_tokens"}
//                                         → {"ok": "<the answer as JSON>"} or {"error": "…"}
// `schema` is the JSON Schema Hysj already sends its other models; it's
// turned into a DynamicGenerationSchema so the answer always fits it.
import Foundation
import FoundationModels

struct Request: Decodable {
    var check: Bool?
    var system: String?
    var user: String?
    var schema: JSONValue?
    var temperature: Double?
    var max_tokens: Int?
}

/// Any JSON (for the schema).
indirect enum JSONValue: Decodable {
    case object([String: JSONValue]), array([JSONValue]), string(String), number(Double), bool(Bool), null
    init(from d: Decoder) throws {
        let c = try d.singleValueContainer()
        if c.decodeNil() { self = .null }
        else if let b = try? c.decode(Bool.self) { self = .bool(b) }
        else if let n = try? c.decode(Double.self) { self = .number(n) }
        else if let s = try? c.decode(String.self) { self = .string(s) }
        else if let a = try? c.decode([JSONValue].self) { self = .array(a) }
        else { self = .object(try c.decode([String: JSONValue].self)) }
    }
    subscript(key: String) -> JSONValue? { if case .object(let o) = self { return o[key] } else { return nil } }
    var string: String? { if case .string(let s) = self { return s } else { return nil } }
    var int: Int? { if case .number(let n) = self { return Int(n) } else { return nil } }
}

func reply(_ value: [String: Any]) {
    let data = try! JSONSerialization.data(withJSONObject: value)
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write("\n".data(using: .utf8)!)
}

/// JSON Schema (the subset Hysj uses) → DynamicGenerationSchema.
func dynamic(_ s: JSONValue, name: String) -> DynamicGenerationSchema {
    if case .array(let options)? = s["enum"] {
        return DynamicGenerationSchema(name: name, anyOf: options.compactMap { $0.string })
    }
    switch s["type"]?.string {
    case "object":
        var props: [DynamicGenerationSchema.Property] = []
        if case .object(let p)? = s["properties"] {
            for key in p.keys.sorted() {
                props.append(.init(name: key, schema: dynamic(p[key]!, name: "\(name)_\(key)")))
            }
        }
        return DynamicGenerationSchema(name: name, properties: props)
    case "array":
        return DynamicGenerationSchema(
            arrayOf: dynamic(s["items"] ?? .object(["type": .string("string")]), name: "\(name)_item"),
            minimumElements: s["minItems"]?.int, maximumElements: s["maxItems"]?.int)
    case "number": return DynamicGenerationSchema(type: Double.self)
    case "integer": return DynamicGenerationSchema(type: Int.self)
    case "boolean": return DynamicGenerationSchema(type: Bool.self)
    default: return DynamicGenerationSchema(type: String.self)
    }
}

@main
struct Main {
    static func main() async {
        let model = SystemLanguageModel.default
        while let line = readLine(strippingNewline: true) {
            guard let req = try? JSONDecoder().decode(Request.self, from: Data(line.utf8)) else {
                reply(["error": "bad request"]); continue
            }
            if req.check == true {
                var reason = ""
                if case .unavailable(let why) = model.availability { reason = String(describing: why) }
                let langs = Set(model.supportedLanguages.compactMap { $0.languageCode?.identifier })
                reply(["available": model.isAvailable, "reason": reason, "languages": langs.sorted()])
                continue
            }
            do {
                let schema = try GenerationSchema(root: dynamic(req.schema ?? .object([:]), name: "Answer"), dependencies: [])
                // A fresh session per request: no memory between emails.
                let session = LanguageModelSession(instructions: req.system ?? "")
                let options = GenerationOptions(
                    sampling: (req.temperature ?? 0) == 0 ? .greedy : nil,
                    temperature: req.temperature,
                    maximumResponseTokens: req.max_tokens)
                let r = try await session.respond(to: req.user ?? "", schema: schema, options: options)
                reply(["ok": r.content.jsonString])
            } catch {
                // Guardrails, context too long, model busy: Hysj shows the subject instead.
                reply(["error": String(describing: error)])
            }
        }
    }
}
