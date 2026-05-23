@@system LinearFsm {
    interface:
        start()
        progress(amount: integer)
        finish()

    machine:
        $Idle {
            start() { -> $Active }
        }

        $Active {
            progress(amount: integer) {
                self.total = self.total + amount
            }
            finish() { -> $Done }
        }

        $Done { }

    domain:
        total: integer = 0
}
