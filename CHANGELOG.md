# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Enhanced performance analytics
- Export functionality for benchmark results

## [1.0.0] - 2025-06-26

### Added
- Initial release of Ollama Rust UI
- Single model testing with Q4 and Q5 support
- Parallel model comparison functionality
- Real-time performance monitoring
- Stress testing with 5 parallel requests
- Smart model pre-loading for improved performance
- 5-second cooldown protection to prevent Ollama overload
- Advanced retry logic with exponential backoff
- Real-time memory monitoring (app and system RAM)
- Performance history tracking (last 20 requests)
- Session statistics with success rates
- Tokens per second calculation
- Native macOS GUI with egui
- Connection validation and testing
- Detailed error handling and recovery

### Technical Features
- Dual-core i5 optimization
- Per-core Tokio runtimes
- HTTP connection pooling
- Zero-cost abstractions
- Thread-safe concurrent operations
- Automatic resource management

### UI/UX
- Smooth 60fps interface
- Real-time status updates
- Visual feedback with spinners and progress indicators
- Collapsible sections for better organization
- Responsive layout design
- Performance stats dashboard

### Performance
- Sub-second startup time
- ~30MB memory footprint
- Millisecond-precision timing
- Automatic performance optimization
- Smart request queuing

## [0.9.0] - 2025-06-25

### Added
- Beta release for testing
- Core functionality implementation
- Basic GUI framework
- HTTP API integration

## [0.1.0] - 2025-06-24

### Added
- Project initialization
- Basic Cargo setup
- Initial dependencies
