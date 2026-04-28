row=055
round=fourth
category=e2e
surface=doctor-bundle
owner=doctor-export
command=cargo test -p yode-tui doctor_bundle_handoff_is_dense
evidence=doctor bundle

surface=doctor-bundle
owner=doctor-export
next=cargo test -p yode-tui print_export_regression_snapshot --quiet
