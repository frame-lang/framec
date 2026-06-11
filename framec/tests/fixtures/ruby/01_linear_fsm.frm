@@system LinearFsm {
    interface:
        start()
        progress(amount: Integer)
        finish()

    machine:
        $Idle {
            start() { -> $Active }
        }

        $Active {
            progress(amount: Integer) {
                @@:self.total = @@:self.total + amount
            }
            finish() { -> $Done }
        }

        $Done { }

    domain:
        total: Integer = 0
}
