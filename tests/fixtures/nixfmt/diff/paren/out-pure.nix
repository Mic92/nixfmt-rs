[
  (
    done
    // listToAttrs [
      {
        # multiline
        name = entry;
        value = 1;
      }
    ]
  )
]
  # Trailing comment in paren in assignment RHS must be idempotent
  {
    o = (
      # c
      ''
        "
      ''
    );
  }
  (
    (
      # test
      a # test
    )
      ((c))
      (
        (c) # e
      )
      (
        (
          c # d
        )
      )
      (
        (
          c # d
        ) # e
      )
      (
        # b
        (c)
      )
      (
        # b
        (c) # e
      )
      (
        # b
        (
          c # d
        )
      )
      (
        # b
        (
          c # d
        ) # e
      )
      # a
      ((c))
      # a
      (
        (c) # e
      )
      # a
      (
        (
          c # d
        )
      )
      # a
      (
        (
          c # d
        ) # e
      )
      # a
      (
        # b
        (c)
      )
      # a
      (
        # b
        (c) # e
      )
      # a
      (
        # b
        (
          c # d
        )
      )
      (
        # a
        # b
        (
          c # d
        ) # e
      )
  )
