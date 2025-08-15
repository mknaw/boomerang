# Boomerang - Scheduled LLM Tool Execution

## Project Overview

A tool that enables scheduled LLM tool-calling sessions with iOS notifications. Users can create natural language schedules like "Every morning, M-F, check my emails and notify me with a summary, only if I received anything important" or "let me know in an hour what the weather is like in <location>".

## Architecture

### Backend
- **Language**: Rust
- **Orchestration**: Temporal for reliable scheduled execution
- **LLM Integration**: Tool-calling LLM sessions
- **DSL**: Internal scheduling specification language (LLM converts natural language to DSL)

### Frontend
- **Platform**: iOS (Swift/SwiftUI)
- **Rationale**: Maximum API access for notifications, background processing, and system integration
- **Alternative considered**: React Native (faster development, cross-platform) - rejected due to need for deep iOS integration

## Key Features
- Natural language schedule creation
- LLM-powered tool execution
- iOS notifications with smart filtering
- Background processing for scheduled tasks
- Tool discovery and management interface

## Technical Requirements
- iOS background app refresh
- Push notifications
- Network requests for backend communication
- Persistent storage for schedules and preferences
