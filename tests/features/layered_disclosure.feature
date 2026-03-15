Feature: Layered disclosure levels
  Files can be veiled at different levels to control how much
  information is disclosed to AI agents.

  Background:
    Given a funveil project is initialized

  Scenario: Level 0 removes file from disk
    Given a file "src/secret.rs" with content:
      """
      pub fn secret() { println!("hidden"); }
      """
    When I veil "src/secret.rs" at level 0
    Then "src/secret.rs" should not exist on disk

  Scenario: Level 1 shows only signatures
    Given a file "src/api.rs" with content:
      """
      pub fn handle_request(req: &str) -> String {
          let processed = req.to_uppercase();
          format!("Response: {}", processed)
      }
      """
    When I veil "src/api.rs" at level 1
    Then "src/api.rs" should exist on disk
    And "src/api.rs" should contain "handle_request"
    And "src/api.rs" should not contain "to_uppercase"

  Scenario: Level 3 is equivalent to full unveil
    Given a file "src/open.rs" with content:
      """
      pub fn open_fn() { println!("visible"); }
      """
    When I veil "src/open.rs"
    And I unveil "src/open.rs" at level 3
    Then "src/open.rs" should exist on disk
    And "src/open.rs" should contain "visible"

  Scenario: Context command unveils related functions
    Given a file "src/entry.rs" with content:
      """
      pub fn entry() { helper(); }
      fn helper() {}
      """
    When I veil "src/entry.rs"
    And I request context for "entry" with depth 1
    Then "src/entry.rs" should exist on disk
