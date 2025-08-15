import Foundation

struct ConfigManager {
    static let shared = ConfigManager()
    
    private let config: [String: Any]
    
    private init() {
        guard let path = Bundle.main.path(forResource: "Config", ofType: "plist"),
              let plist = NSDictionary(contentsOfFile: path) as? [String: Any] else {
            fatalError("Could not load Config.plist")
        }
        self.config = plist
    }
    
    func string(for key: String) -> String? {
        if let envValue = ProcessInfo.processInfo.environment[key] {
            print("🌍 Using environment variable \(key): \(envValue)")
            return envValue
        }
        if let plistValue = config[key] as? String {
            print("📄 Using plist value \(key): \(plistValue)")
            return plistValue
        }
        return nil
    }
    
    func requiredString(for key: String) -> String {
        guard let value = string(for: key) else {
            fatalError("Required configuration key '\(key)' not found")
        }
        return value
    }
    
    var apiBaseURL: String {
        let url = requiredString(for: "APIBaseURL")
        print("🔧 Config loaded APIBaseURL: \(url)")
        return url
    }
    
    var apiVersion: String {
        return requiredString(for: "APIVersion")
    }
    
    var environment: String {
        return requiredString(for: "Environment")
    }
    
    var isProduction: Bool {
        return environment.lowercased() == "production"
    }
    
    var isDevelopment: Bool {
        return environment.lowercased() == "development"
    }
}