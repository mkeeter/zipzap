function z.pwd --on-variable PWD
  status --is-command-substitution
    and return
  zipzap --quiet add $PWD &
end
