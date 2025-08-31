# Boomerang - Scheduled LLM Tool Execution

## Project Overview

A tool that enables scheduled LLM tool-calling sessions with iOS notifications. Users can create natural language schedules like "Every morning, M-F, check my emails and notify me with a summary, only if I received anything important" or "let me know in an hour what the weather is like in <location>".

## Architecture

This repository contains the backend components only. The iOS frontend is maintained in a separate repository.

### Backend
- **Language**: Rust
- **Orchestration**: Temporal for reliable scheduled execution
- **LLM Integration**: Tool-calling LLM sessions
- **DSL**: Internal scheduling specification language (LLM converts natural language to DSL)
- **Components**:
  - **Server**: Main HTTP API server
  - **Agent**: LLM tool execution engine

## Key Features
- Natural language schedule creation
- LLM-powered tool execution
- iOS notifications with smart filtering
- Background processing for scheduled tasks
- Tool discovery and management interface

## Backend Technical Requirements
- Rust async runtime (tokio)
- Temporal workflow orchestration
- HTTP API endpoints for schedule management
- LLM provider integration (OpenAI)
- Configuration management
- Tool execution framework
