// Sentence vectors from Apple's NaturalLanguage framework (macOS 14+): a
// multilingual transformer that ships with macOS (assets fetched by the OS
// on first use), no Apple Intelligence needed. Shared by main and stub:
//   {"embed": ["text", …]} → {"vectors": [[Float], …], "model": "<id>"} or {"error": "…"}
import Foundation
import NaturalLanguage

nonisolated(unsafe) private var loaded: NLContextualEmbedding?

/// The Latin-script model (English, Norwegian, Swedish, Danish, …), loaded once.
private func model() async throws -> NLContextualEmbedding {
    if let m = loaded { return m }
    guard let m = NLContextualEmbedding(script: .latin) else {
        throw NSError(domain: "hysj", code: 1, userInfo: [NSLocalizedDescriptionKey: "no Latin-script embedding model"])
    }
    if !m.hasAvailableAssets {
        _ = try await m.requestAssets()
    }
    try m.load()
    loaded = m
    return m
}

/// Mean of the token vectors, unit length. Long texts are cut (the model
/// reads at most `maximumSequenceLength` tokens anyway).
func embed(_ texts: [String]) async -> [String: Any] {
    do {
        let m = try await model()
        var out: [[Float]] = []
        for full in texts {
            let t = String(full.prefix(1500))
            var sum = [Double](repeating: 0, count: m.dimension)
            var n = 0
            if !t.isEmpty {
                let r = try m.embeddingResult(for: t, language: nil)
                r.enumerateTokenVectors(in: t.startIndex..<t.endIndex) { v, _ in
                    for i in 0..<min(v.count, sum.count) { sum[i] += v[i] }
                    n += 1
                    return true
                }
            }
            let norm = sqrt(sum.reduce(0) { $0 + $1 * $1 })
            out.append(sum.map { Float(norm > 0 ? $0 / norm : 0) })
        }
        return ["vectors": out, "model": "nl-latin-\(m.modelIdentifier)"]
    } catch {
        return ["error": String(describing: error)]
    }
}
