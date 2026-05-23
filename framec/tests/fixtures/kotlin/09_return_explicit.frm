@@system ReturnExplicit {
    interface:
        decide(score: Int): String

    machine:
        $Judging {
            decide(score: Int): String {
                if score >= 60 {
                    @@:return("pass")
                }
                @@:return("fail")
            }
        }
}
