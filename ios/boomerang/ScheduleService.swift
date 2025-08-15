import Foundation

struct Schedule: Codable, Identifiable {
    let id: UUID
    let name: String
    let description: String
    let schedule: String
    let isActive: Bool
    let createdAt: String
    let updatedAt: String
}

struct CreateScheduleRequest: Codable {
    let name: String
    let description: String
    let schedule: String
}

typealias ScheduleListResponse = [Schedule]

class ScheduleService {
    private let apiService = APIService.shared
    
    func getSchedules() async throws -> [Schedule] {
        let response: ScheduleListResponse = try await apiService.request(
            endpoint: "/schedules",
            responseType: ScheduleListResponse.self
        )
        return response
    }
    
    func createSchedule(name: String, description: String, schedule: String) async throws -> Schedule {
        let request = CreateScheduleRequest(
            name: name,
            description: description,
            schedule: schedule
        )
        
        let requestData = try JSONEncoder().encode(request)
        
        return try await apiService.request(
            endpoint: "/schedules",
            method: .POST,
            body: requestData,
            responseType: Schedule.self
        )
    }
    
    func deleteSchedule(id: UUID) async throws {
        try await apiService.request(
            endpoint: "/schedules/\(id)",
            method: .DELETE
        )
    }
    
    func toggleSchedule(id: UUID, isActive: Bool) async throws -> Schedule {
        let requestBody = ["isActive": isActive]
        let requestData = try JSONSerialization.data(withJSONObject: requestBody)
        
        return try await apiService.request(
            endpoint: "/schedules/\(id)",
            method: .PATCH,
            body: requestData,
            responseType: Schedule.self
        )
    }
}
