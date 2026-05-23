@@system ReturnExplicit {
    interface:
        decide(score: integer): string

    machine:
        $Judging {
            decide(score: integer): string {
                if score >= 60 {
                    @@:return("pass")
                }
                @@:return("fail")
            }
        }
}
