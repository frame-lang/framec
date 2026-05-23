@@system LinearFsm {
    interface:
        start()
        progress(amount: int)
        finish()

    machine:
        $Idle {
            start() { -> $Active }
        }

        $Active {
            progress(amount: int) {
                self.total = self.total + amount
            }
            finish() { -> $Done }
        }

        $Done { }

    domain:
        total: int = 0
}
