import Foundation

struct NetworkConfig {
    static let shared = NetworkConfig()
    
    private let configManager = ConfigManager.shared
    
    private init() {}
    
    var baseURL: String {
        return configManager.apiBaseURL
    }
    
    var apiVersion: String {
        return configManager.apiVersion
    }
    
    var fullBaseURL: String {
        return baseURL
    }
    
    var isProduction: Bool {
        return configManager.isProduction
    }
}