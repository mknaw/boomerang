import SwiftUI

struct ContentView: View {
    @State private var schedules: [Schedule] = []
    @State private var isLoading = false
    @State private var errorMessage: String?
    
    private let scheduleService = ScheduleService()
    
    var body: some View {
        NavigationView {
            VStack {
                if isLoading {
                    ProgressView("Loading schedules...")
                } else if schedules.isEmpty {
                    Text("No schedules yet")
                        .foregroundColor(.secondary)
                } else {
                    List(schedules) { schedule in
                        VStack(alignment: .leading, spacing: 4) {
                            Text(schedule.name)
                                .font(.headline)
                            Text(schedule.description)
                                .font(.subheadline)
                                .foregroundColor(.secondary)
                            Text("Schedule: \(schedule.schedule)")
                                .font(.caption)
                                .foregroundColor(.blue)
                        }
                        .padding(.vertical, 2)
                    }
                }
                
                if let errorMessage = errorMessage {
                    Text("Error: \(errorMessage)")
                        .foregroundColor(.red)
                        .padding()
                }
                
                Button("Test API Connection") {
                    Task {
                        await testConnection()
                    }
                }
                .padding()
            }
            .navigationTitle("Boomerang")
            .task {
                await loadSchedules()
            }
        }
    }
    
    private func loadSchedules() async {
        isLoading = true
        errorMessage = nil
        
        do {
            schedules = try await scheduleService.getSchedules()
        } catch {
            errorMessage = error.localizedDescription
        }
        
        isLoading = false
    }
    
    private func testConnection() async {
        do {
            let testSchedule = try await scheduleService.createSchedule(
                name: "Test Schedule",
                description: "This is a test schedule",
                schedule: "0 9 * * 1-5"
            )
            print("Created test schedule: \(testSchedule)")
            await loadSchedules()
        } catch {
            errorMessage = "Connection test failed: \(error.localizedDescription)"
        }
    }
}

#Preview {
    ContentView()
}
