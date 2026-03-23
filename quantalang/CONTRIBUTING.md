# Contributing to QuantaLang

Thank you for your interest in contributing to QuantaLang! This document provides guidelines and instructions for contributing.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Making Changes](#making-changes)
- [Pull Request Process](#pull-request-process)
- [Coding Standards](#coding-standards)
- [Testing](#testing)
- [Documentation](#documentation)

## Code of Conduct

By participating in this project, you agree to abide by our [Code of Conduct](CODE_OF_CONDUCT.md). Please read it before contributing.

## Getting Started

### Types of Contributions

We welcome many types of contributions:

- 🐛 **Bug Reports**: Found a bug? Open an issue!
- 📝 **Documentation**: Improve docs, fix typos, add examples
- 💡 **Feature Requests**: Suggest new features via issues
- 🔧 **Code**: Fix bugs, implement features, improve performance
- 🧪 **Tests**: Add test cases, improve coverage
- 🌍 **Translations**: Help translate documentation

### Finding Issues to Work On

- Look for issues labeled [`good first issue`](https://github.com/quantalang/quantalang/labels/good%20first%20issue) for beginner-friendly tasks
- [`help wanted`](https://github.com/quantalang/quantalang/labels/help%20wanted) issues need community help
- Check the [project board](https://github.com/quantalang/quantalang/projects) for planned work

## Development Setup

### Prerequisites

- Git
- A C compiler (GCC or Clang)
- LLVM 15+ (for code generation)
- Python 3.8+ (for build scripts)

### Clone and Build

```bash
# Clone the repository
git clone https://github.com/quantalang/quantalang.git
cd quantalang

# Install dependencies
./scripts/install-deps.sh

# Build in debug mode
./build.sh debug

# Run tests to verify setup
./test.sh
```

### Project Structure

```
quantalang/
├── src/
│   ├── compiler/       # Compiler implementation
│   │   ├── lexer/      # Tokenization
│   │   ├── parser/     # Parsing
│   │   ├── ast/        # Abstract syntax tree
│   │   ├── hir/        # High-level IR
│   │   ├── mir/        # Mid-level IR
│   │   ├── codegen/    # Code generation
│   │   └── optimize/   # Optimization passes
│   ├── std/            # Standard library
│   ├── runtime/        # Runtime support
│   └── tools/          # Tooling (formatter, linter, etc.)
├── tests/              # Test suite
├── benchmarks/         # Performance benchmarks
├── docs/               # Documentation
└── examples/           # Example programs
```

## Making Changes

### Workflow

1. **Fork** the repository
2. **Create a branch** from `main`:
   ```bash
   git checkout -b feature/my-feature
   # or
   git checkout -b fix/issue-123
   ```
3. **Make your changes**
4. **Test your changes**:
   ```bash
   ./test.sh
   ```
5. **Commit** with a descriptive message
6. **Push** to your fork
7. **Open a Pull Request**

### Branch Naming

- `feature/description` - New features
- `fix/issue-number` or `fix/description` - Bug fixes
- `docs/description` - Documentation changes
- `refactor/description` - Code refactoring
- `perf/description` - Performance improvements

### Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): description

[optional body]

[optional footer]
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation
- `style`: Formatting (no code change)
- `refactor`: Code restructuring
- `perf`: Performance improvement
- `test`: Adding tests
- `chore`: Maintenance

Examples:
```
feat(parser): add support for async functions
fix(codegen): correct handling of nested loops
docs(stdlib): add examples for HashMap
```

## Pull Request Process

### Before Submitting

- [ ] Code compiles without warnings
- [ ] All tests pass (`./test.sh`)
- [ ] Code is formatted (`quanta fmt`)
- [ ] Linter passes (`quanta lint`)
- [ ] Documentation is updated if needed
- [ ] Commit messages follow conventions

### PR Description

Include:
1. **What** changes were made
2. **Why** the changes are needed
3. **How** to test the changes
4. Related issue numbers (e.g., "Fixes #123")

### Review Process

1. Automated CI checks run
2. Maintainers review the code
3. Address review feedback
4. Maintainer approves and merges

## Coding Standards

### General Guidelines

- Write clear, self-documenting code
- Prefer explicit over implicit
- Keep functions focused and small
- Use meaningful names
- Add comments for complex logic

### Formatting

Use the built-in formatter:

```bash
quanta fmt src/
```

Key rules:
- 4 spaces for indentation
- 100 character line limit
- Trailing newline at end of files
- No trailing whitespace

### Naming Conventions

| Item | Convention | Example |
|------|------------|---------|
| Functions | snake_case | `parse_expression` |
| Types | PascalCase | `AstNode` |
| Constants | SCREAMING_SNAKE | `MAX_BUFFER_SIZE` |
| Variables | snake_case | `token_count` |
| Type Parameters | Single uppercase | `T`, `E` |
| Modules | snake_case | `type_checker` |

### Error Handling

- Use `Result<T, E>` for recoverable errors
- Use `panic!` only for unrecoverable bugs
- Provide helpful error messages
- Include context in errors

```quanta
// Good
fn parse_number(s: &str) -> Result<i64, ParseError> {
    s.parse().map_err(|e| ParseError::InvalidNumber {
        input: s.to_string(),
        cause: e,
    })
}

// Bad
fn parse_number(s: &str) -> i64 {
    s.parse().unwrap()  // Don't do this!
}
```

## Testing

### Running Tests

```bash
# All tests
./test.sh

# Unit tests only
./test.sh unit

# Integration tests
./test.sh integration

# Specific test
./test.sh path/to/test.quanta

# With verbose output
./test.sh -v
```

### Writing Tests

```quanta
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_parsing() {
        let ast = parse("1 + 2");
        assert!(ast.is_ok());
    }
    
    #[test]
    fn test_error_handling() {
        let result = parse("1 +");
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("unexpected end"));
    }
    
    #[test]
    #[should_panic(expected = "out of bounds")]
    fn test_panic_case() {
        let v = vec![1, 2, 3];
        let _ = v[10];
    }
}
```

### Test Organization

- Unit tests go in the same file as the code
- Integration tests go in `tests/`
- Performance tests go in `benchmarks/`

## Documentation

### Code Documentation

Document all public items:

```quanta
/// Parses a source file into an AST.
///
/// # Arguments
///
/// * `source` - The source code to parse
/// * `filename` - The filename for error messages
///
/// # Returns
///
/// Returns the parsed AST, or an error if parsing fails.
///
/// # Examples
///
/// ```
/// let ast = parse("fn main() {}", "main.quanta")?;
/// ```
///
/// # Errors
///
/// Returns `ParseError` if the source contains syntax errors.
pub fn parse(source: &str, filename: &str) -> Result<Ast, ParseError> {
    // ...
}
```

### Documentation Files

- Use Markdown for documentation
- Place in `docs/` directory
- Include code examples
- Keep language simple and clear

### Building Documentation

```bash
# Generate API docs
quanta doc src/ -o docs/api/

# Build documentation site
mkdocs build

# Serve locally
mkdocs serve
```

## Getting Help

- **Discord**: [discord.gg/quantalang](https://discord.gg/quantalang)
- **Forum**: [forum.quantalang.org](https://forum.quantalang.org)
- **Issues**: Open a GitHub issue

## Recognition

Contributors are recognized in:
- [CONTRIBUTORS.md](CONTRIBUTORS.md)
- Release notes
- Annual contributor spotlight

Thank you for contributing to QuantaLang! 🎉
