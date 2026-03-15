Feature: Token budget mode
  Agents can request a disclosure plan within a token budget,
  prioritizing focus files and their dependencies.

  Background:
    Given a funveil project is initialized

  Scenario: Disclosure plan respects token budget
    Given a file "src/focus.rs" with content:
      """
      pub fn focus_fn() { dep_fn(); }
      """
    And a file "src/dep.rs" with content:
      """
      pub fn dep_fn() {}
      """
    When I veil "src/focus.rs"
    And I veil "src/dep.rs"
    And I request a disclosure plan with budget 10000 focused on "src/focus.rs"
    Then the plan should have total tokens within budget

  Scenario: Focus file gets highest disclosure level
    Given a file "src/main_focus.rs" with content:
      """
      pub fn main_focus() {}
      """
    When I veil "src/main_focus.rs"
    And I request a disclosure plan with budget 10000 focused on "src/main_focus.rs"
    Then the plan should include "src/main_focus.rs" at level 3

  Scenario: Empty budget produces empty plan
    Given a file "src/any.rs" with content:
      """
      pub fn any() {}
      """
    When I veil "src/any.rs"
    And I request a disclosure plan with budget 0 focused on "src/any.rs"
    Then the plan should have 0 entries
