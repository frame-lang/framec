@@system LifecycleArgs {
    interface:
        load(n: int, label: str)
        total(): int
        tag(): str

    machine:
        $Idle {
            load(n: int, label: str) {
                -> (n, label) $Active
            }
        }

        $Active {
            $>(count: int, name: str) {
                self.sum = count + 1;
                self.label = name;
            }
            total(): int {
                @@:(self.sum)
                return
            }
            tag(): str {
                @@:(self.label)
                return
            }
        }

    domain:
        sum: int = 0
        label: str = ""
}
