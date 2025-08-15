import Foundation

class APIService {
    static let shared = APIService()
    
    private let session: URLSession
    private let config = NetworkConfig.shared
    
    private init() {
        let configuration = URLSessionConfiguration.default
        configuration.timeoutIntervalForRequest = 30
        configuration.timeoutIntervalForResource = 60
        self.session = URLSession(configuration: configuration)
    }
    
    enum APIError: Error {
        case invalidURL
        case noData
        case decodingError(Error)
        case networkError(Error)
        case serverError(Int)
    }
    
    func request<T: Codable>(
        endpoint: String,
        method: HTTPMethod = .GET,
        body: Data? = nil,
        headers: [String: String] = [:],
        responseType: T.Type
    ) async throws -> T {
        let fullURL = "\(config.fullBaseURL)\(endpoint)"
        print("🌐 Making request to: \(fullURL)")
        
        guard let url = URL(string: fullURL) else {
            throw APIError.invalidURL
        }
        
        var request = URLRequest(url: url)
        request.httpMethod = method.rawValue
        request.httpBody = body
        
        var defaultHeaders = [
            "Content-Type": "application/json",
            "Accept": "application/json"
        ]
        
        for (key, value) in headers {
            defaultHeaders[key] = value
        }
        
        for (key, value) in defaultHeaders {
            request.setValue(value, forHTTPHeaderField: key)
        }
        
        do {
            let (data, response) = try await session.data(for: request)
            
            guard let httpResponse = response as? HTTPURLResponse else {
                throw APIError.networkError(URLError(.badServerResponse))
            }
            
            guard 200...299 ~= httpResponse.statusCode else {
                throw APIError.serverError(httpResponse.statusCode)
            }
            
            guard !data.isEmpty else {
                throw APIError.noData
            }
            
            do {
                let decodedResponse = try JSONDecoder().decode(responseType, from: data)
                return decodedResponse
            } catch {
                throw APIError.decodingError(error)
            }
        } catch {
            if error is APIError {
                throw error
            } else {
                throw APIError.networkError(error)
            }
        }
    }
    
    func request(
        endpoint: String,
        method: HTTPMethod = .GET,
        body: Data? = nil,
        headers: [String: String] = [:]
    ) async throws {
        guard let url = URL(string: "\(config.fullBaseURL)\(endpoint)") else {
            throw APIError.invalidURL
        }
        
        var request = URLRequest(url: url)
        request.httpMethod = method.rawValue
        request.httpBody = body
        
        var defaultHeaders = [
            "Content-Type": "application/json",
            "Accept": "application/json"
        ]
        
        for (key, value) in headers {
            defaultHeaders[key] = value
        }
        
        for (key, value) in defaultHeaders {
            request.setValue(value, forHTTPHeaderField: key)
        }
        
        do {
            let (_, response) = try await session.data(for: request)
            
            guard let httpResponse = response as? HTTPURLResponse else {
                throw APIError.networkError(URLError(.badServerResponse))
            }
            
            guard 200...299 ~= httpResponse.statusCode else {
                throw APIError.serverError(httpResponse.statusCode)
            }
        } catch {
            if error is APIError {
                throw error
            } else {
                throw APIError.networkError(error)
            }
        }
    }
}

enum HTTPMethod: String {
    case GET = "GET"
    case POST = "POST"
    case PUT = "PUT"
    case DELETE = "DELETE"
    case PATCH = "PATCH"
}
